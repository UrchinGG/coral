use std::collections::HashMap;

use anyhow::Result;
use blacklist::EMOTE_TAG;
use serenity::all::*;

use database::{
    BlacklistRepository, EvidenceKind, PublishedAttachment, ReviewError, ReviewRepository,
    ReviewStatus,
};

use super::render::{self, MediaUrls, PostState};
use super::{ForumTags, Loaded, custom_id, deadline, face_attachment, face_filename, load_live};
use crate::framework::Data;
use crate::interact;
use crate::utils::discord_media::message_media;
use crate::utils::{separator, text};

const CONFIRMED_REFUSAL: &str =
    "This player is a confirmed cheater. Only staff can change a confirmed tag.";

pub struct TagAddRequest<'a> {
    pub submitter_id: u64,
    pub player_uuid: &'a str,
    pub player_name: &'a str,
    pub tag_type: &'a str,
    pub reason: &'a str,
    pub hide_username: bool,
}

pub enum TagAddRoute {
    Console(Vec<CreateComponent<'static>>),
    Confirmation(Vec<CreateComponent<'static>>),
    Refused(&'static str),
    OpenReview(Vec<CreateComponent<'static>>),
}

pub async fn route_tag_add(
    ctx: &Context,
    data: &Data,
    request: TagAddRequest<'_>,
    current_tag: Option<(String, String, String)>,
) -> Result<TagAddRoute> {
    if is_confirmed(data, request.player_uuid).await? {
        return Ok(TagAddRoute::Refused(CONFIRMED_REFUSAL));
    }
    let repo = ReviewRepository::new(data.db.pool());
    if let Some(open) = repo.open_for_player(request.player_uuid).await? {
        return Ok(TagAddRoute::OpenReview(open_review_notice(
            request.player_name,
            thread_link(data, open.thread_id),
        )));
    }
    if let Some(draft) = repo
        .draft_for(request.player_uuid, request.submitter_id as i64)
        .await?
        && !repo
            .expire_idle_draft(draft.id, super::draft_idle_window())
            .await?
    {
        return Ok(TagAddRoute::Console(
            super::console::console_view(ctx, data, draft.id, None).await?,
        ));
    }
    let draft = repo
        .create_draft(
            request.player_uuid,
            request.submitter_id as i64,
            request.hide_username,
            request.tag_type,
            request.reason,
        )
        .await?;
    deadline::arm(ctx.clone(), data.clone(), draft.id, deadline::Kind::Draft);
    Ok(TagAddRoute::Confirmation(confirmation(
        &request,
        draft.id,
        current_tag,
    )))
}

fn confirmation(
    request: &TagAddRequest<'_>,
    review_id: i64,
    current_tag: Option<(String, String, String)>,
) -> Vec<CreateComponent<'static>> {
    use super::super::channel::format_tag_block;
    use crate::utils::sanitize_reason;

    let proposed = format_tag_block(
        request.tag_type,
        &sanitize_reason(request.reason),
        "",
        None,
        None,
        false,
    );
    let start = CreateButton::new(custom_id("start", review_id))
        .label("Start Draft")
        .style(ButtonStyle::Success);
    let face = format!("attachment://{}", face_filename(request.player_uuid));
    let mut parts = vec![text(format!("## {EMOTE_TAG} Tag Review"))];
    match current_tag {
        Some((tag_type, detail, added_line)) => {
            parts.push(text(
                "This player already has a tag in the same group. Your review opens as a private \
                 draft where you add evidence; nothing is posted until you submit. If it is \
                 approved, your tag replaces the current one.",
            ));
            parts.push(separator());
            parts.push(text("-# Current"));
            parts.push(face_section(
                format!(
                    "IGN - `{}`\n\n{}",
                    request.player_name,
                    format_tag_block(&tag_type, &detail, "", Some(&added_line), None, false)
                ),
                &face,
            ));
            parts.push(separator());
            parts.push(text("-# Proposed"));
            parts.push(CreateContainerComponent::Section(CreateSection::new(
                vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                    proposed,
                ))],
                CreateSectionAccessory::Button(start),
            )));
        }
        None => {
            parts.push(text(
                "This tag needs approval first. Your review opens as a private draft where you add \
                 evidence; nothing is posted until you submit.",
            ));
            parts.push(separator());
            parts.push(text("-# Preview"));
            parts.push(face_section(
                format!("IGN - `{}`\n\n{proposed}", request.player_name),
                &face,
            ));
            parts.push(separator());
            parts.push(CreateContainerComponent::ActionRow(
                CreateActionRow::buttons(vec![start]),
            ));
        }
    }
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn face_section(content: String, face: &str) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            content,
        ))],
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
            face.to_string(),
        ))),
    ))
}

pub async fn submit(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    loaded: Loaded,
) -> Result<()> {
    if loaded.review.status != ReviewStatus::Draft {
        return Ok(());
    }
    if !loaded
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::Replay)
    {
        return super::console::show_console(ctx, component, data, loaded.review.id, None).await;
    }
    if is_confirmed(data, &loaded.review.player_uuid).await? {
        return interact::edit_component(ctx, component, render::simple(CONFIRMED_REFUSAL)).await;
    }
    let repo = ReviewRepository::new(data.db.pool());
    let claimed = match repo.claim_open(loaded.review.id).await {
        Ok(review) => review,
        Err(ReviewError::OpenReviewExists) => {
            let open = repo.open_for_player(&loaded.review.player_uuid).await?;
            return interact::edit_component(
                ctx,
                component,
                someone_published_first(&loaded, thread_link(data, open.and_then(|r| r.thread_id))),
            )
            .await;
        }
        Err(_) => {
            return interact::edit_component(
                ctx,
                component,
                render::simple("This draft expired. Run /tag add again to start a new one."),
            )
            .await;
        }
    };
    let loaded = Loaded {
        review: claimed,
        ..loaded
    };
    let result = create_post(ctx, data, &loaded).await;
    let reply = match result {
        Ok(thread_id) => {
            tokio::spawn(ping_reviewers(ctx.clone(), data.clone(), thread_id));
            render::simple(format!(
                "## {EMOTE_TAG} Review Submitted\nYour review of `{}` is open for voting in <#{thread_id}>.",
                loaded.username
            ))
        }
        Err(e) => {
            tracing::error!("Failed to publish review {}: {e:?}", loaded.review.id);
            repo.release_claim(loaded.review.id).await?;
            super::console::console_view(
                ctx,
                data,
                loaded.review.id,
                Some("The review could not be posted. Nothing was published; try again."),
            )
            .await?
        }
    };
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(reply)
                .new_attachment(face_attachment(data, &loaded.review.player_uuid).await),
        )
        .await?;
    Ok(())
}

async fn create_post(ctx: &Context, data: &Data, loaded: &Loaded) -> Result<ThreadId> {
    let Some(forum_id) = data.review_forum_id else {
        anyhow::bail!("review forum is not configured");
    };
    let repo = ReviewRepository::new(data.db.pool());
    let staged = repo
        .staged_media(loaded.review.id, loaded.revision.revision)
        .await?;
    let post = render::public_post(
        loaded,
        &PostState {
            tally: Default::default(),
            votes: &[],
            paused: false,
            edited_at: None,
            rules: crate::utils::standing::rules(data).await?,
        },
        &MediaUrls::attached(),
    );
    let mut message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(post)
        .add_file(face_attachment(data, &loaded.review.player_uuid).await);
    for media in &staged {
        message = message.add_file(CreateAttachment::bytes(
            media.bytes.clone(),
            media.filename.clone(),
        ));
    }
    let tags = ForumTags::resolve(ctx, data).await;
    let forum_post = tags.for_status(ReviewStatus::Open).into_iter().fold(
        CreateForumPost::new(loaded.username.clone(), message),
        |post, tag| post.add_applied_tag(tag),
    );
    let thread = forum_id.create_forum_post(&ctx.http, forum_post).await?;

    let attachments =
        match message_media(&ctx.http, thread.id.into(), MessageId::new(thread.id.get())).await {
            Ok(media) => published_attachments(&staged, &media, thread.id.get() as i64),
            Err(e) => {
                tracing::error!(
                    "Could not read media ids for review {}: {e}",
                    loaded.review.id
                );
                Vec::new()
            }
        };
    repo.finish_publish(loaded.review.id, thread.id.get() as i64, &attachments)
        .await?;
    Ok(thread.id)
}

pub fn published_attachments(
    staged: &[database::StagedMedia],
    media: &[crate::utils::discord_media::MessageMedia],
    message_id: i64,
) -> Vec<PublishedAttachment> {
    staged
        .iter()
        .filter_map(|file| {
            media
                .iter()
                .find(|m| m.in_gallery && m.filename == file.filename)
                .map(|m| PublishedAttachment {
                    evidence_id: file.evidence_id,
                    message_id,
                    attachment_id: m.attachment_id.get() as i64,
                })
        })
        .collect()
}

pub async fn rerender_post(ctx: &Context, data: &Data, review_id: i64) {
    if let Err(e) = try_rerender_post(ctx, data, review_id).await {
        tracing::error!("Failed to re-render review {review_id}: {e:?}");
    }
}

pub async fn try_rerender_post(ctx: &Context, data: &Data, review_id: i64) -> Result<()> {
    let Some(loaded) = load_live(ctx, data, review_id).await? else {
        return Ok(());
    };
    let Some(thread_id) = loaded.review.thread_id else {
        return Ok(());
    };
    let repo = ReviewRepository::new(data.db.pool());
    let tally = repo.tally(review_id).await?;
    let votes = repo.active_votes(review_id).await?;
    let paused = repo.edit_session(review_id).await?.is_some();
    let edited_at = (repo.revision_count(review_id).await? > 1)
        .then_some(loaded.revision.published_at)
        .flatten();

    let channel_id = GenericChannelId::new(thread_id as u64);
    let message_id = MessageId::new(thread_id as u64);
    let media = message_media(&ctx.http, channel_id, message_id).await?;
    let by_attachment: HashMap<u64, String> = media
        .iter()
        .map(|m| (m.attachment_id.get(), m.url.clone()))
        .collect();
    let urls = MediaUrls {
        evidence: loaded
            .evidence
            .iter()
            .filter_map(|e| {
                let url = by_attachment.get(&(e.attachment_id? as u64))?;
                Some((e.id, url.clone()))
            })
            .collect(),
        face: media
            .iter()
            .find(|m| !m.in_gallery && m.filename == face_filename(&loaded.review.player_uuid))
            .map(|m| m.url.clone()),
    };
    let components = render::public_post(
        &loaded,
        &PostState {
            tally,
            votes: &votes,
            paused,
            edited_at,
            rules: crate::utils::standing::rules(data).await?,
        },
        &urls,
    );
    let mut keep = EditAttachments::new();
    for item in &media {
        keep = keep.keep(item.attachment_id);
    }
    ctx.http
        .edit_message(
            channel_id,
            message_id,
            &EditMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(components)
                .attachments(keep),
            Vec::new(),
        )
        .await?;
    Ok(())
}

pub async fn set_forum_tags(ctx: &Context, data: &Data, thread_id: i64, status: ReviewStatus) {
    let tags = ForumTags::resolve(ctx, data).await;
    if let Err(e) = ThreadId::new(thread_id as u64)
        .edit(
            &ctx.http,
            EditThread::new().applied_tags(tags.for_status(status)),
        )
        .await
    {
        tracing::warn!("Failed to set forum tags on review thread {thread_id}: {e}");
    }
}

async fn ping_reviewers(ctx: Context, data: Data, thread_id: ThreadId) {
    let (review_role, _, _) = database::ReviewGuideRepository::new(data.db.pool())
        .get_ping_roles()
        .await
        .unwrap_or((None, None, None));
    let mut lines = Vec::new();
    if let Some(role_id) = review_role {
        lines.push(format!("<@&{role_id}>"));
    }
    lines.push(
        ">>> Vote on the validity of the above tag. If the player is not cheating, reject it. \
         If the player is cheating but the tag type or reason is wrong, request a revision."
            .to_string(),
    );
    let notice = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            vec![text(lines.join("\n"))],
        ))]);
    if let Err(e) = ctx
        .http
        .send_message(thread_id.into(), Vec::<CreateAttachment>::new(), &notice)
        .await
    {
        tracing::warn!("Failed to post the review notice in {thread_id}: {e}");
    }
}

async fn is_confirmed(data: &Data, player_uuid: &str) -> Result<bool> {
    Ok(BlacklistRepository::new(data.db.pool())
        .get_active_tag(player_uuid, "confirmed_cheater")
        .await?
        .is_some())
}

fn open_review_notice(player_name: &str, link: Option<String>) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![text(format!(
        "## {EMOTE_TAG} Tag Review\nA review of `{player_name}` is already open. You can vote on it, \
         or share your evidence in its thread."
    ))];
    if let Some(link) = link {
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::buttons(vec![CreateButton::new_link(link).label("Open Review")]),
        ));
    }
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn someone_published_first(loaded: &Loaded, link: Option<String>) -> Vec<CreateComponent<'static>> {
    let mut buttons = Vec::new();
    if let Some(link) = link {
        buttons.push(CreateButton::new_link(link).label("Open Review"));
    }
    buttons.push(
        CreateButton::new(custom_id("discard", loaded.review.id))
            .label("Discard")
            .style(ButtonStyle::Danger),
    );
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(format!(
            "## {EMOTE_TAG} Tag Review\nSomeone else published a review for `{}` while you were \
             drafting, so yours was not posted.\nYou can still vote on it, or share your evidence \
             in its thread.",
            loaded.username
        )),
        CreateContainerComponent::ActionRow(CreateActionRow::buttons(buttons)),
    ]))]
}

pub fn thread_link(data: &Data, thread_id: Option<i64>) -> Option<String> {
    let guild_id = data.home_guild_id?;
    Some(format!(
        "https://discord.com/channels/{guild_id}/{}",
        thread_id?
    ))
}
