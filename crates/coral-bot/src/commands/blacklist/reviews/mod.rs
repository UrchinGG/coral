mod builder;
mod compose;
mod evidence;
mod state;
mod verdict;

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serenity::all::*;

use crate::framework::Data;

pub use builder::{CurrentTag, FACE_SIZE, build_confirmation_message, face_filename};
pub use compose::{
    handle_add_player, handle_addplayer_name_modal, handle_addplayer_reason_modal,
    handle_edit_done, handle_edit_reason, handle_edit_reason_modal, handle_edit_submitted,
    handle_edit_tag, handle_pending_tag_select, handle_remove_player, handle_tag_select_edit,
};
pub use evidence::{
    handle_add_replay, handle_attach_media, handle_edit_replay, handle_edit_replay_modal,
    handle_evidence_select, handle_media_modal, handle_remove_evidence, handle_replay_modal,
};
pub use verdict::{
    handle_abort_delete, handle_approve, handle_cancel_thread, handle_confirm, handle_reject,
    handle_reject_modal, handle_submit,
};

use state::*;

const TAG_PENDING: &str = "Pending";
const TAG_APPROVED: &str = "Approved";
const TAG_REJECTED: &str = "Rejected";
const TAG_AWAITING_EVIDENCE: &str = "Awaiting Evidence";

const MAX_MEDIA_PER_PLAYER: usize = 4;
const ALLOWED_MEDIA_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm", "mov"];
pub const REVIEW_TAGS: &[&str] = &["closet_cheater", "blatant_cheater"];
const CONFIRMABLE_TAGS: &[&str] = &["closet_cheater", "blatant_cheater"];
const SUBMISSION_TIMEOUT_SECS: u64 = 30 * 60;
const SUBMISSION_WARNING_SECS: u64 = 20 * 60;

pub const ACCEPT_THRESHOLD: usize = 6;
pub const REJECT_THRESHOLD: usize = 3;

async fn player_face_attachments(
    data: &Data,
    state: &SubmissionState,
) -> Vec<CreateAttachment<'static>> {
    let mut out = Vec::with_capacity(state.players.len());
    for player in &state.players {
        let png = data
            .skin_provider
            .fetch_face(&player.uuid, builder::FACE_SIZE)
            .await
            .unwrap_or_else(default_face_png);
        out.push(CreateAttachment::bytes(
            png,
            builder::face_filename(&player.uuid),
        ));
    }
    out
}

fn default_face_png() -> Vec<u8> {
    let size = builder::FACE_SIZE;
    let img = image::RgbaImage::from_pixel(size, size, image::Rgba([0, 0, 0, 0]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn build_tag_select_options(selected: Option<&str>) -> Vec<CreateSelectMenuOption<'static>> {
    blacklist::all()
        .iter()
        .filter(|def| REVIEW_TAGS.contains(&def.name))
        .map(|def| {
            let mut opt = CreateSelectMenuOption::new(def.display_name, def.name);
            if selected == Some(def.name) {
                opt = opt.default_selection(true);
            }
            opt
        })
        .collect()
}

fn extract_modal_value(modal: &ModalInteraction, field_id: &str) -> String {
    crate::interact::extract_modal_value(&modal.data.components, field_id)
}

fn extract_text_displays(message: &Message) -> Vec<String> {
    let Some(container) = message.components.iter().find_map(|c| match c {
        Component::Container(c) => Some(c),
        _ => None,
    }) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for c in &*container.components {
        match c {
            ContainerComponent::TextDisplay(td) => {
                if let Some(content) = &td.content {
                    out.push(content.clone());
                }
            }
            ContainerComponent::Section(section) => {
                for sc in &*section.components {
                    if let SectionComponent::TextDisplay(td) = sc {
                        if let Some(content) = &td.content {
                            out.push(content.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn find_container(message: &Message) -> Option<&serenity::all::Container> {
    message.components.iter().find_map(|c| match c {
        Component::Container(c) => Some(c),
        _ => None,
    })
}

fn parse_component_ids(custom_id: &str) -> (usize, u64) {
    let mut parts = custom_id.split(':');
    let _ = parts.next();
    let player_idx = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let submitter_id = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (player_idx, submitter_id)
}

fn parse_submitter_id(custom_id: &str) -> Option<u64> {
    custom_id.split(':').last()?.parse().ok()
}

fn is_submitter(component: &ComponentInteraction) -> bool {
    parse_submitter_id(&component.data.custom_id).unwrap_or(0) == component.user.id.get()
}

async fn require_submitter(ctx: &Context, component: &ComponentInteraction) -> Result<bool> {
    if is_submitter(component) {
        return Ok(true);
    }
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Only the submission creator can use these buttons")
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(false)
}

async fn send_vote_error(
    ctx: &Context,
    component: &ComponentInteraction,
    message: &str,
) -> Result<()> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(message)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

fn thread_id(channel_id: GenericChannelId) -> ThreadId {
    ThreadId::new(channel_id.get())
}

pub(crate) fn reset_thread_votes(data: &Data, thread_id: u64) {
    data.pending_review_votes.lock().unwrap().remove(&thread_id);
    data.vote_messages
        .lock()
        .unwrap()
        .retain(|(t, _, _), _| *t != thread_id);
}

fn attachment_id_from_cdn_url(url: &str) -> Option<AttachmentId> {
    let path = url.split("/attachments/").nth(1)?;
    let id_str = path.split('/').nth(1)?;
    id_str
        .split('?')
        .next()
        .unwrap_or(id_str)
        .parse::<u64>()
        .ok()
        .map(AttachmentId::new)
}

async fn find_builder_message(ctx: &Context, channel_id: GenericChannelId) -> Option<Message> {
    ctx.http
        .get_message(channel_id, MessageId::new(channel_id.get()))
        .await
        .ok()
}

async fn send_thread_message(
    ctx: &Context,
    channel_id: GenericChannelId,
    content: &str,
) -> Result<()> {
    ctx.http
        .send_message(
            channel_id,
            Vec::<CreateAttachment>::new(),
            &CreateMessage::new().content(content),
        )
        .await?;
    Ok(())
}

fn thread_title(state: &SubmissionState) -> String {
    state
        .players
        .iter()
        .map(|p| p.username.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

async fn fetch_replaced(
    ctx: &Context,
    data: &Data,
    state: &SubmissionState,
) -> HashMap<String, builder::ReplacedTag> {
    let repo = database::BlacklistRepository::new(data.db.pool());
    let mut map = HashMap::new();
    for player in &state.players {
        if player.status != PlayerStatus::Pending {
            continue;
        }
        let Ok(tags) = repo.get_active_tags(&player.uuid).await else {
            continue;
        };
        let lane = blacklist::priority_lane(&player.tag_type);
        let Some(existing) = tags.iter().find(|t| {
            let tt = t.tag_type.as_deref().unwrap_or("");
            tt != player.tag_type && lane.iter().any(|l| l == tt)
        }) else {
            continue;
        };
        map.insert(
            player.uuid.clone(),
            builder::ReplacedTag {
                tag_type: existing.tag_type.clone().unwrap_or_default(),
                reason: existing.reason.clone().unwrap_or_default(),
                added_line: super::channel::format_added_line(ctx, existing).await,
            },
        );
    }
    map
}

async fn update_builder(
    ctx: &Context,
    data: &Data,
    channel_id: GenericChannelId,
    message: &Message,
    state: &SubmissionState,
) -> Result<()> {
    let existing_urls = gallery_url_map(message);
    let replaced = fetch_replaced(ctx, data, state).await;
    let submitter = super::channel::get_username(ctx, state.submitter_id).await;
    let faces = player_face_attachments(data, state).await;

    let mut attachments = EditAttachments::new();
    for url in existing_urls.values() {
        if let Some(id) = attachment_id_from_cdn_url(url) {
            attachments = attachments.keep(id);
        }
    }
    for f in &faces {
        attachments = attachments.add(f.clone());
    }

    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(builder::build_review_message(
            state,
            &existing_urls,
            &replaced,
            &submitter,
        ))
        .attachments(attachments);
    ctx.http
        .edit_message(channel_id, message.id, &edit, faces)
        .await?;
    let thread_id = thread_id(channel_id);
    let _ = thread_id
        .edit(&ctx.http, EditThread::new().name(thread_title(state)))
        .await;
    Ok(())
}

fn gallery_url_map(message: &Message) -> HashMap<String, String> {
    let Some(container) = find_container(message) else {
        return HashMap::new();
    };

    let mut map = HashMap::new();
    for part in &*container.components {
        if let ContainerComponent::MediaGallery(gallery) = part {
            for item in &*gallery.items {
                let url = item.media.url.to_string();
                if !url.starts_with("attachment://") {
                    map.insert(attachment_filename_from_url(&url), url);
                }
            }
        }
    }
    map
}

async fn update_builder_with_files(
    ctx: &Context,
    data: &Data,
    channel_id: GenericChannelId,
    message: &Message,
    state: &SubmissionState,
    files: Vec<CreateAttachment<'static>>,
) -> Result<()> {
    let existing_urls = gallery_url_map(message);
    let replaced = fetch_replaced(ctx, data, state).await;
    let submitter = super::channel::get_username(ctx, state.submitter_id).await;
    let faces = player_face_attachments(data, state).await;

    let mut attachments = EditAttachments::new();
    for url in existing_urls.values() {
        if let Some(id) = attachment_id_from_cdn_url(url) {
            attachments = attachments.keep(id);
        }
    }
    for f in files.iter().cloned() {
        attachments = attachments.add(f);
    }
    for f in &faces {
        attachments = attachments.add(f.clone());
    }

    let mut all_files = files;
    all_files.extend(faces);

    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(builder::build_review_message(
            state,
            &existing_urls,
            &replaced,
            &submitter,
        ))
        .attachments(attachments);
    ctx.http
        .edit_message(channel_id, message.id, &edit, all_files)
        .await?;
    Ok(())
}

async fn update_builder_keep_media(
    ctx: &Context,
    data: &Data,
    channel_id: GenericChannelId,
    message: &Message,
    state: &SubmissionState,
) -> Result<()> {
    let existing_urls = gallery_url_map(message);
    let replaced = fetch_replaced(ctx, data, state).await;
    let keep: HashSet<&str> = state
        .players
        .iter()
        .flat_map(|p| p.evidence.iter())
        .filter_map(|e| match e {
            Evidence::Attachment { filename } => Some(filename.as_str()),
            _ => None,
        })
        .collect();
    let submitter = super::channel::get_username(ctx, state.submitter_id).await;
    let faces = player_face_attachments(data, state).await;

    let mut attachments = EditAttachments::new();
    for (fname, url) in &existing_urls {
        if keep.contains(fname.as_str()) {
            if let Some(id) = attachment_id_from_cdn_url(url) {
                attachments = attachments.keep(id);
            }
        }
    }
    for f in &faces {
        attachments = attachments.add(f.clone());
    }

    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(builder::build_review_message(
            state,
            &existing_urls,
            &replaced,
            &submitter,
        ))
        .attachments(attachments);
    ctx.http
        .edit_message(channel_id, message.id, &edit, faces)
        .await?;
    Ok(())
}

fn attachment_filename_from_url(url: &str) -> String {
    if let Some(name) = url.strip_prefix("attachment://") {
        return name.to_string();
    }
    url.rsplit('/')
        .next()
        .map(|s| s.split('?').next().unwrap_or(s))
        .unwrap_or("unknown.png")
        .to_string()
}

async fn resolve_forum_tags(ctx: &Context, data: &Data) -> ForumTags {
    let empty = ForumTags {
        pending: None,
        approved: None,
        rejected: None,
        awaiting_evidence: None,
    };

    let Some(forum_id) = data.review_forum_id else {
        return empty;
    };
    let Ok(channel) = ctx.http.get_channel(forum_id.into()).await else {
        return empty;
    };
    let Channel::Guild(gc) = channel else {
        return empty;
    };

    let find = |name: &str| {
        gc.available_tags
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.id)
    };
    ForumTags {
        pending: find(TAG_PENDING),
        approved: find(TAG_APPROVED),
        rejected: find(TAG_REJECTED),
        awaiting_evidence: find(TAG_AWAITING_EVIDENCE),
    }
}

async fn set_forum_tags(ctx: &Context, thread_id: ThreadId, tag_ids: &[ForumTagId]) -> Result<()> {
    thread_id
        .edit(&ctx.http, EditThread::new().applied_tags(tag_ids.to_vec()))
        .await?;
    Ok(())
}

pub async fn create_submission(
    ctx: &Context,
    data: &Data,
    submitter_id: u64,
    player_name: &str,
    player_uuid: &str,
    tag_type: &str,
    reason: &str,
) -> Result<ThreadId> {
    let Some(forum_id) = data.review_forum_id else {
        anyhow::bail!("Review forum channel not configured");
    };

    if !REVIEW_TAGS.contains(&tag_type) && !CONFIRMABLE_TAGS.contains(&tag_type) {
        anyhow::bail!("Tag type '{}' cannot be submitted for review", tag_type);
    }

    let player = PlayerEntry {
        username: player_name.to_string(),
        uuid: player_uuid.to_string(),
        tag_type: tag_type.to_string(),
        reason: reason.to_string(),
        status: PlayerStatus::Pending,
        review_note: None,
        evidence: Vec::new(),
        accept_votes: Vec::new(),
        reject_votes: Vec::new(),
        reviewer_names: Vec::new(),
    };

    let state = SubmissionState {
        submitter_id,
        players: vec![player],
        submitted: false,
        reopened: false,
        editing: None,
        editing_evidence: 0,
        pending_add: None,
    };

    let replaced = fetch_replaced(ctx, data, &state).await;
    let submitter = super::channel::get_username(ctx, submitter_id).await;
    let faces = player_face_attachments(data, &state).await;
    let mut message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(builder::build_review_message(
            &state,
            &HashMap::new(),
            &replaced,
            &submitter,
        ));
    for f in faces {
        message = message.add_file(f);
    }

    let mut forum_post = CreateForumPost::new(player_name.to_string(), message);
    let tags = resolve_forum_tags(ctx, data).await;
    if let Some(tag_id) = tags.awaiting_evidence {
        forum_post = forum_post.add_applied_tag(tag_id);
    }

    let thread = forum_id.create_forum_post(&ctx.http, forum_post).await?;

    let reminder = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(builder::build_submit_reminder(submitter_id));
    let _ = ctx
        .http
        .send_message(thread.id.into(), Vec::<CreateAttachment>::new(), &reminder)
        .await;

    Ok(thread.id)
}

pub fn spawn_submission_timeout(ctx: Context, thread_id: ThreadId) {
    let channel_id: GenericChannelId = thread_id.into();

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(SUBMISSION_WARNING_SECS)).await;

        let Some(msg) = find_builder_message(&ctx, channel_id).await else {
            return;
        };
        let Some(state) = parse_state_from_message(&msg) else {
            return;
        };
        if state.submitted {
            return;
        }

        let _ = send_thread_message(
            &ctx, channel_id,
            &format!(
                "<@{}> This submission will be automatically cancelled in 10 minutes due to inactivity.",
                state.submitter_id
            ),
        ).await;

        tokio::time::sleep(std::time::Duration::from_secs(
            SUBMISSION_TIMEOUT_SECS - SUBMISSION_WARNING_SECS,
        ))
        .await;

        let Some(msg) = find_builder_message(&ctx, channel_id).await else {
            return;
        };
        let Some(state) = parse_state_from_message(&msg) else {
            return;
        };
        if state.submitted {
            return;
        }

        let _ = channel_id.delete(&ctx.http, None).await;
    });
}
