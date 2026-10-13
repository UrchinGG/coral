use anyhow::Result;
use blacklist::lookup as lookup_tag;
use serenity::all::*;

use database::{
    BlacklistRepository, EvidenceKind, EvidencePostRepository, Review, ReviewGuideRepository,
    ReviewRepository,
};

use super::super::evidence_post::confirm::{Reply, Target, confirm_flow};
use super::super::evidence_post::{self, media::current_media, menu::add_sources, render};
use super::{REVIEW_TAGS, custom_id, face_attachment, face_filename};
use crate::commands::blacklist::tag::get_rank;
use crate::framework::{AccessRank, Data};
use crate::interact;
use crate::utils::discord_media::message_media;
use crate::utils::{resolve_username_or_fetch, text};

const HEADING: &str = "## Accepted With Media";
const QUESTION: &str = "Confirm this tag and create an evidence post?";
const CONFIRMED_TAG: &str = "confirmed_cheater";

enum PlayerState {
    Confirmed,
    Untagged,
    HasPost,
    Ready,
}

pub async fn ping_if_media(ctx: &Context, data: &Data, review: &Review) -> bool {
    post_ping(ctx, data, review).await.unwrap_or_else(|e| {
        tracing::warn!(
            "Failed to post the confirm ping in review {}: {e}",
            review.id
        );
        false
    })
}

pub async fn handle_click(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review_id: i64,
) -> Result<()> {
    let staff = component.user.id.get();
    if get_rank(data, staff).await? < AccessRank::Helper {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only helpers and above can use this",
        )
        .await;
    }
    component.defer_ephemeral(&ctx.http).await?;
    let Some(review) = ReviewRepository::new(data.db.pool()).get(review_id).await? else {
        return reply_error(ctx, component, "That review no longer exists").await;
    };
    let uuid = evidence_post::normalize(&review.player_uuid);
    let target = Target {
        username: resolve_username_or_fetch(&uuid, data)
            .await
            .unwrap_or_else(|| uuid.clone()),
        uuid,
    };
    let state = player_state(data, &target.uuid).await?;
    let view = match confirm_flow(ctx, data, &target, None).await? {
        Reply::Error(message) => {
            return reply_error(ctx, component, &message).await;
        }
        Reply::View(view) => view,
    };
    let face = Some(evidence_post::face_attachment(data, &target.uuid).await);
    match state {
        PlayerState::Untagged => reply(ctx, component, view, face).await,
        PlayerState::Confirmed | PlayerState::HasPost => {
            reply(ctx, component, view, face).await?;
            edit_notice(ctx, data, component, &review, None).await;
            Ok(())
        }
        PlayerState::Ready => finish(ctx, component, data, &review, &target, staff).await,
    }
}

async fn finish(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    target: &Target,
    staff: u64,
) -> Result<()> {
    let Some(post) = EvidencePostRepository::new(data.db.pool())
        .canonical(&target.uuid)
        .await?
    else {
        return reply_error(ctx, component, "The evidence post could not be found").await;
    };
    let summary = match media_sources(ctx, data, review).await {
        Ok(sources) => add_sources(ctx, data, post.thread_id, &sources, staff).await,
        Err(message) => message,
    };
    let copied = current_media(ctx, post.thread_id)
        .await
        .is_ok_and(|media| media.iter().any(|m| m.in_gallery));
    let mut lines = vec![summary.lines().collect::<Vec<_>>().join(" ")];
    if !copied {
        lines.push("The tag is not confirmed yet. Finish with Add Media on the post.".to_string());
    }
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let view = render::notice(
        &format!("{} Evidence Post Created", confirmed_emote()),
        &target.username,
        Some(&evidence_post::thread_link(data, post.thread_id)),
        &lines,
    );
    let face = Some(evidence_post::face_attachment(data, &target.uuid).await);
    reply(ctx, component, view, face).await?;

    let outcome = if is_confirmed(data, &target.uuid).await? {
        "Confirmed by"
    } else {
        "Evidence post created by"
    };
    let line = format!("-# {outcome} <@{staff}> in <#{}>.", post.thread_id);
    edit_notice(ctx, data, component, review, Some(line)).await;
    Ok(())
}

async fn post_ping(ctx: &Context, data: &Data, review: &Review) -> Result<bool> {
    let Some(thread_id) = review.thread_id else {
        return Ok(false);
    };
    let repo = ReviewRepository::new(data.db.pool());
    let Some(revision) = repo.live_revision(review.id).await? else {
        return Ok(false);
    };
    if !REVIEW_TAGS.contains(&revision.tag_type.as_str()) {
        return Ok(false);
    }
    let has_media = repo
        .evidence(review.id, revision.revision)
        .await?
        .iter()
        .any(|e| e.kind == EvidenceKind::Media);
    let uuid = evidence_post::normalize(&review.player_uuid);
    if !has_media
        || is_confirmed(data, &uuid).await?
        || EvidencePostRepository::new(data.db.pool())
            .canonical(&uuid)
            .await?
            .is_some()
    {
        return Ok(false);
    }
    let face = format!("attachment://{}", face_filename(&review.player_uuid));
    let button = CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
        CreateButton::new(custom_id("confirmtag", review.id))
            .label("Confirm Tag")
            .style(ButtonStyle::Primary),
    ]));
    let message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(notice_components(
            confirm_role(data).await,
            face,
            &format!("{HEADING}\n{QUESTION}"),
            Some(button),
        ))
        .add_file(face_attachment(data, &review.player_uuid).await);
    ctx.http
        .send_message(
            GenericChannelId::new(thread_id as u64),
            Vec::<CreateAttachment>::new(),
            &message,
        )
        .await?;
    Ok(true)
}

async fn edit_notice(
    ctx: &Context,
    data: &Data,
    component: &ComponentInteraction,
    review: &Review,
    line: Option<String>,
) {
    let channel_id = component.channel_id;
    let message_id = component.message.id;
    let face = message_media(&ctx.http, channel_id, message_id)
        .await
        .ok()
        .and_then(|media| media.into_iter().find(|m| !m.in_gallery))
        .map_or_else(
            || format!("attachment://{}", face_filename(&review.player_uuid)),
            |m| m.url,
        );
    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(notice_components(
            confirm_role(data).await,
            face,
            HEADING,
            line.map(text),
        ));
    let thread = ThreadId::new(channel_id.get());
    set_thread_closed(ctx, thread, false).await;
    if let Err(e) = ctx
        .http
        .edit_message(channel_id, message_id, &edit, Vec::new())
        .await
    {
        tracing::warn!(
            "Failed to update the confirm ping in review {}: {e}",
            review.id
        );
    }
    set_thread_closed(ctx, thread, true).await;
}

async fn set_thread_closed(ctx: &Context, thread: ThreadId, closed: bool) {
    if let Err(e) = thread
        .edit(&ctx.http, EditThread::new().archived(closed).locked(closed))
        .await
    {
        tracing::warn!("Failed to set review thread {thread} closed={closed}: {e}");
    }
}

fn notice_components(
    role: Option<i64>,
    face: String,
    body: &str,
    footer: Option<CreateContainerComponent<'static>>,
) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            body.to_string(),
        ))],
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(face))),
    ))];
    parts.extend(footer);
    let mut components: Vec<CreateComponent<'static>> = role
        .map(|id| CreateComponent::TextDisplay(CreateTextDisplay::new(format!("<@&{id}>"))))
        .into_iter()
        .collect();
    components.push(CreateComponent::Container(CreateContainer::new(parts)));
    components
}

async fn media_sources(
    ctx: &Context,
    data: &Data,
    review: &Review,
) -> Result<Vec<(String, String)>, String> {
    let unreadable = || "The review's media could not be read, so nothing was added.".to_string();
    let repo = ReviewRepository::new(data.db.pool());
    let revision = repo
        .live_revision(review.id)
        .await
        .ok()
        .flatten()
        .ok_or_else(unreadable)?;
    let evidence = repo
        .evidence(review.id, revision.revision)
        .await
        .map_err(|_| unreadable())?;
    let thread_id = review.thread_id.ok_or_else(unreadable)? as u64;
    let posted = message_media(
        &ctx.http,
        GenericChannelId::new(thread_id),
        MessageId::new(thread_id),
    )
    .await
    .map_err(|_| unreadable())?;

    let mut sources = Vec::new();
    let mut missing = Vec::new();
    for item in evidence.iter().filter(|e| e.kind == EvidenceKind::Media) {
        let name = item.filename.clone().unwrap_or_default();
        let found = item.attachment_id.and_then(|id| {
            posted
                .iter()
                .find(|m| m.in_gallery && m.attachment_id.get() == id as u64)
        });
        match found {
            Some(media) => sources.push((name, media.url.clone())),
            None => missing.push(name),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "These files could not be found on the review, so nothing was added: {}",
            missing.join(", ")
        ));
    }
    if sources.is_empty() {
        return Err("The review has no media to copy.".to_string());
    }
    Ok(sources)
}

async fn player_state(data: &Data, uuid: &str) -> Result<PlayerState> {
    if is_confirmed(data, uuid).await? {
        return Ok(PlayerState::Confirmed);
    }
    let tags = BlacklistRepository::new(data.db.pool())
        .get_active_tags(uuid)
        .await?;
    let tagged = tags.iter().any(|t| {
        t.tag_type
            .as_deref()
            .is_some_and(|ty| REVIEW_TAGS.contains(&ty))
    });
    if !tagged {
        return Ok(PlayerState::Untagged);
    }
    let has_post = EvidencePostRepository::new(data.db.pool())
        .canonical(uuid)
        .await?
        .is_some();
    Ok(if has_post {
        PlayerState::HasPost
    } else {
        PlayerState::Ready
    })
}

async fn is_confirmed(data: &Data, uuid: &str) -> Result<bool> {
    Ok(BlacklistRepository::new(data.db.pool())
        .get_active_tag(uuid, CONFIRMED_TAG)
        .await?
        .is_some())
}

async fn confirm_role(data: &Data) -> Option<i64> {
    ReviewGuideRepository::new(data.db.pool())
        .get_ping_roles()
        .await
        .ok()
        .and_then(|(_, _, confirm)| confirm)
}

async fn reply(
    ctx: &Context,
    component: &ComponentInteraction,
    view: Vec<CreateComponent<'static>>,
    face: Option<CreateAttachment<'static>>,
) -> Result<()> {
    let mut edit = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(view);
    if let Some(face) = face {
        edit = edit.new_attachment(face);
    }
    component.edit_response(&ctx.http, edit).await?;
    Ok(())
}

async fn reply_error(ctx: &Context, component: &ComponentInteraction, message: &str) -> Result<()> {
    reply(
        ctx,
        component,
        vec![interact::error_container("Error", message)],
        None,
    )
    .await
}

fn confirmed_emote() -> &'static str {
    lookup_tag(CONFIRMED_TAG).map_or("", |d| d.emote)
}
