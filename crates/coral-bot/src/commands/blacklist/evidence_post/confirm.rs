use anyhow::Result;
use blacklist::lookup as lookup_tag;
use serenity::all::*;

use database::{
    BlacklistRepository, EvidencePost, EvidencePostRepository, NewPost, Origin, PendingTag,
    PostStatus, ReviewRepository, set_pending,
};

use super::convert::Conversion;
use super::render::{self, face_section};
use super::{face_attachment, normalize, ops, thread_link, thread_title};
use crate::commands::blacklist::tag::get_rank;
use crate::framework::{AccessRank, Data};
use crate::interact;
use crate::utils::text;

const BASE_TAGS: &[&str] = &["closet_cheater", "blatant_cheater"];
const REASON_MAX_CHARS: u16 = 120;

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("confirm")
        .description("Create an evidence post and confirm a cheater tag")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "player", "Player name or UUID")
                .required(true),
        )
}

pub enum Reply {
    View(Vec<CreateComponent<'static>>),
    Error(String),
}

pub struct Target {
    pub uuid: String,
    pub username: String,
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;
    if get_rank(data, command.user.id.get()).await? < AccessRank::Helper {
        return interact::send_deferred_error(
            ctx,
            command,
            "Error",
            "Only helpers and above can use /confirm",
        )
        .await;
    }
    let query = command
        .data
        .options()
        .iter()
        .find_map(|o| match (o.name, &o.value) {
            ("player", ResolvedValue::String(s)) => Some(s.to_string()),
            _ => None,
        })
        .unwrap_or_default();
    let Ok(info) = data.api.resolve(&query).await else {
        return interact::send_deferred_error(ctx, command, "Error", "Player not found").await;
    };
    let target = Target {
        uuid: normalize(&info.uuid),
        username: info.username.clone(),
    };
    let reply = confirm_flow(ctx, data, &target, None).await?;
    match reply {
        Reply::Error(message) => {
            interact::send_deferred_error(ctx, command, "Error", &message).await
        }
        Reply::View(view) => {
            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .flags(MessageFlags::IS_COMPONENTS_V2)
                        .components(view)
                        .new_attachment(face_attachment(data, &target.uuid).await),
                )
                .await?;
            Ok(())
        }
    }
}

pub async fn confirm_flow(
    ctx: &Context,
    data: &Data,
    target: &Target,
    pending: Option<PendingTag>,
) -> Result<Reply> {
    let tags = BlacklistRepository::new(data.db.pool())
        .get_active_tags(&target.uuid)
        .await?;
    let holds = |wanted: &[&str]| {
        tags.iter()
            .any(|t| t.tag_type.as_deref().is_some_and(|ty| wanted.contains(&ty)))
    };
    if holds(&["confirmed_cheater"]) {
        return Ok(Reply::View(already_confirmed(data, target).await));
    }
    if let Some(open) = ReviewRepository::new(data.db.pool())
        .open_for_player(&target.uuid)
        .await?
    {
        let mut message = format!(
            "A review of `{}` is open. Add your evidence to that review and approve it instead.",
            target.username
        );
        if let Some(link) = super::super::review::publish::thread_link(data, open.thread_id) {
            message.push_str(&format!("\nThread: {link}"));
        }
        return Ok(Reply::Error(message));
    }
    if !holds(BASE_TAGS) && pending.is_none() {
        return Ok(Reply::View(tag_info_screen(target)));
    }

    let repo = EvidencePostRepository::new(data.db.pool());
    let mut post = repo.canonical(&target.uuid).await?;
    if let Some(existing) = &post
        && existing.status == Some(PostStatus::Confirmed)
    {
        if let Err(e) = ops::archive(ctx, data, existing.thread_id).await {
            tracing::error!(
                "Archiving stale evidence post {} failed: {e}",
                existing.thread_id
            );
        }
        post = repo.get(existing.thread_id).await?;
    }
    let pending = pending.filter(|_| !holds(BASE_TAGS));
    match post {
        None => create_post(ctx, data, target, pending).await,
        Some(post) => {
            if let Some(pending) = &pending {
                let mut tx = repo.begin_locked(&target.uuid).await?;
                set_pending(&mut tx, post.thread_id, Some(pending)).await?;
                tx.commit().await?;
            }
            Ok(Reply::View(existing_post_reply(data, target, &post)))
        }
    }
}

async fn create_post(
    ctx: &Context,
    data: &Data,
    target: &Target,
    pending: Option<PendingTag>,
) -> Result<Reply> {
    let Some(forum_id) = data.evidence_forum_id else {
        return Ok(Reply::Error(
            "Evidence forum channel not configured".to_string(),
        ));
    };
    let repo = EvidencePostRepository::new(data.db.pool());
    let mut tx = repo.begin_locked(&target.uuid).await?;
    let title = thread_title(&target.username, &target.uuid);
    let draft = EvidencePost {
        thread_id: 0,
        uuid: target.uuid.clone(),
        origin: Origin::Coral,
        status: Some(PostStatus::Pending),
        title: title.clone(),
        pending: pending.clone(),
        created_at: chrono::Utc::now(),
    };
    let tag = render::tag_view(ctx, data, &draft).await?;
    let thread = forum_id
        .create_forum_post(
            &ctx.http,
            CreateForumPost::new(
                title.clone(),
                CreateMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(render::live(&target.username, &target.uuid, &tag, &[]))
                    .add_file(face_attachment(data, &target.uuid).await),
            ),
        )
        .await?;
    let thread_id = thread.id.get() as i64;
    repo.insert(NewPost {
        thread_id,
        uuid: &target.uuid,
        origin: Origin::Coral,
        status: Some(PostStatus::Pending),
        title: &title,
        created_at: *thread.id.created_at(),
    })
    .await?;
    if let Some(pending) = &pending {
        set_pending(&mut tx, thread_id, Some(pending)).await?;
    }
    tx.commit().await?;
    post_earlier_evidence(ctx, data, &target.uuid, thread.id).await;
    Ok(Reply::View(render::notice(
        &format!("{} Evidence Post Created", confirmed_emote()),
        &target.username,
        Some(&format!("<#{thread_id}>")),
        &[],
    )))
}

async fn post_earlier_evidence(ctx: &Context, data: &Data, uuid: &str, thread_id: ThreadId) {
    let legacy = EvidencePostRepository::new(data.db.pool())
        .legacy_for(uuid)
        .await
        .unwrap_or_default();
    if legacy.is_empty() {
        return;
    }
    let lines: Vec<String> = legacy
        .iter()
        .map(|post| {
            format!(
                "<#{}> <t:{}:D>",
                post.thread_id,
                ThreadId::new(post.thread_id as u64)
                    .created_at()
                    .unix_timestamp()
            )
        })
        .collect();
    let message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            vec![text(format!(
                "## Earlier Evidence\nThis player has evidence posts in an old format.\n{}",
                lines.join("\n")
            ))],
        ))]);
    if let Err(e) = ctx
        .http
        .send_message(thread_id.into(), Vec::<CreateAttachment>::new(), &message)
        .await
    {
        tracing::warn!("Failed to link earlier evidence in {thread_id}: {e}");
    }
}

async fn already_confirmed(data: &Data, target: &Target) -> Vec<CreateComponent<'static>> {
    match super::link_target(data, &target.uuid).await {
        Some(post) => render::notice(
            &format!("{} Player Already Confirmed", confirmed_emote()),
            &target.username,
            Some(&thread_link(data, post.thread_id)),
            &[],
        ),
        None => render::notice(
            &format!("{} Player Already Confirmed", confirmed_emote()),
            &target.username,
            None,
            &["No evidence post exists for this player."],
        ),
    }
}

fn existing_post_reply(
    data: &Data,
    target: &Target,
    post: &EvidencePost,
) -> Vec<CreateComponent<'static>> {
    let link = thread_link(data, post.thread_id);
    if post.status != Some(PostStatus::Archived) {
        return render::notice(
            &format!("{} Evidence Post Already Exists", confirmed_emote()),
            &target.username,
            Some(&link),
            &[],
        );
    }
    let mut view = render::notice(
        &format!("{} Evidence Post Archived", confirmed_emote()),
        &target.username,
        Some(&link),
        &["Restoring reopens the post. If it holds media, the tag is confirmed again."],
    );
    if let Some(CreateComponent::Container(container)) = view.pop() {
        view.push(CreateComponent::Container(container.add_component(
            CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                CreateButton::new(format!("evidence_restore:{}", post.thread_id))
                    .label("Restore Post")
                    .style(ButtonStyle::Primary),
            ])),
        )));
    }
    view
}

fn tag_info_screen(target: &Target) -> Vec<CreateComponent<'static>> {
    let options: Vec<CreateSelectMenuOption> = BASE_TAGS
        .iter()
        .filter_map(|name| lookup_tag(name))
        .map(|def| CreateSelectMenuOption::new(def.display_name, def.name))
        .collect();
    vec![CreateComponent::Container(CreateContainer::new(vec![
        face_section(format!(
            "## Add Tag\nIGN - `{}`\nSelect a tag type to add:",
            target.username
        )),
        CreateContainerComponent::ActionRow(CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("evc_type:{}:{}", target.uuid, target.username),
                CreateSelectMenuKind::String {
                    options: options.into(),
                },
            )
            .placeholder("Tag type..."),
        )),
    ]))]
}

pub async fn handle_type_select(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if get_rank(data, component.user.id.get()).await? < AccessRank::Helper {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only helpers and above can use /confirm",
        )
        .await;
    }
    let Some((uuid, username)) = component
        .data
        .custom_id
        .strip_prefix("evc_type:")
        .and_then(|rest| rest.split_once(':'))
    else {
        return Ok(());
    };
    let tag_type = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().map(|v| v.to_string())
        }
        _ => None,
    };
    let Some(tag_type) = tag_type.filter(|t| BASE_TAGS.contains(&t.as_str())) else {
        return Ok(());
    };
    let lane = blacklist::priority_lane(&tag_type);
    let conflicting = BlacklistRepository::new(data.db.pool())
        .get_active_tags(uuid)
        .await?
        .iter()
        .any(|t| {
            t.tag_type
                .as_deref()
                .is_some_and(|ty| lane.iter().any(|l| l == ty))
        });
    if conflicting {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "Conflicts with an existing tag",
        )
        .await;
    }
    let display = lookup_tag(&tag_type).map_or(tag_type.as_str(), |d| d.display_name);
    let reason = CreateInputText::new(InputTextStyle::Paragraph, "reason")
        .placeholder("Reason for this tag")
        .max_length(REASON_MAX_CHARS)
        .required(true);
    let modal = CreateModal::new(
        format!("evc_reason:{uuid}:{tag_type}:{username}"),
        format!("Add {display} Tag"),
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        "Reason", reason,
    ))]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_reason_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let staff = modal.user.id.get();
    if get_rank(data, staff).await? < AccessRank::Helper {
        return interact::send_modal_error(
            ctx,
            modal,
            "Error",
            "Only helpers and above can use /confirm",
        )
        .await;
    }
    let Some(rest) = modal.data.custom_id.strip_prefix("evc_reason:") else {
        return Ok(());
    };
    let mut parts = rest.splitn(3, ':');
    let (Some(uuid), Some(tag_type), Some(username)) = (parts.next(), parts.next(), parts.next())
    else {
        return Ok(());
    };
    let reason = interact::extract_modal_value(&modal.data.components, "reason");
    let target = Target {
        uuid: uuid.to_string(),
        username: username.to_string(),
    };
    let pending = PendingTag {
        tag_type: tag_type.to_string(),
        reason: reason.trim().to_string(),
        by: staff as i64,
    };
    modal
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let view = match confirm_flow(ctx, data, &target, Some(pending)).await? {
        Reply::View(view) => view,
        Reply::Error(message) => vec![interact::error_container("Error", &message)],
    };
    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(view)
                .new_attachment(face_attachment(data, &target.uuid).await),
        )
        .await?;
    Ok(())
}

pub async fn handle_restore(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let staff = component.user.id.get();
    if get_rank(data, staff).await? < AccessRank::Helper {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only staff can restore evidence",
        )
        .await;
    }
    let Some(thread_id) = component
        .data
        .custom_id
        .strip_prefix("evidence_restore:")
        .and_then(|raw| raw.parse::<i64>().ok())
    else {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "This button no longer works. Run /confirm again.",
        )
        .await;
    };
    let Some(post) = EvidencePostRepository::new(data.db.pool())
        .get(thread_id)
        .await?
    else {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "That evidence post no longer exists",
        )
        .await;
    };
    if post.status != Some(PostStatus::Archived) {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "That evidence post is not archived.",
        )
        .await;
    }
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let username = ops::username(data, &post).await;
    let line = match ops::restore(ctx, data, thread_id, staff).await {
        Ok(Conversion::Converted | Conversion::AlreadyConfirmed) => {
            "The post holds media, so the tag is confirmed again.".to_string()
        }
        Ok(Conversion::NotApplicable) => {
            "The tag is confirmed when media is added to the post.".to_string()
        }
        Ok(Conversion::Failed(cause)) => cause,
        Err(e) => e.to_string(),
    };
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(render::notice(
                    &format!("{} Evidence Post Restored", confirmed_emote()),
                    &username,
                    Some(&thread_link(data, thread_id)),
                    &[&line],
                ))
                .new_attachment(face_attachment(data, &post.uuid).await),
        )
        .await?;
    Ok(())
}

fn confirmed_emote() -> &'static str {
    lookup_tag("confirmed_cheater").map_or("", |d| d.emote)
}
