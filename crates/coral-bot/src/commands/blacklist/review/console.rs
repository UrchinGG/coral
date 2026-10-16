use anyhow::Result;
use blacklist::parse_replay;
use serenity::all::*;

use database::{ReviewRepository, ReviewStatus, VoteKind};

use super::render::{self, ConsoleMode};
use super::{
    ALLOWED_MEDIA_EXTENSIONS, Loaded, MAX_FILES_PER_UPLOAD, MAX_MEDIA, NOTE_MAX_CHARS,
    REASON_MAX_CHARS, REVIEW_TAGS, custom_id, custom_id_with, deadline, draft_idle_window,
    edit_idle_window, face_attachment, load_working, parse_id,
};
use crate::framework::Data;
use crate::interact;

const EXPIRED_DRAFT: &str = "This draft expired. Run /tag add again to start a new one.";
const NOT_YOURS: &str = "Only the submitter can use this.";
const MODAL_ACTIONS: &[&str] = &["replay", "media", "evnote", "evreplay", "reason"];

pub async fn handle_component(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let Some(id) = parse_id(&component.data.custom_id) else {
        return Ok(());
    };
    let opens_modal = MODAL_ACTIONS.contains(&id.action);
    if !opens_modal {
        acknowledge(ctx, component).await?;
    }
    let Some(loaded) = authorized(ctx, data, id.review_id, component.user.id).await? else {
        if opens_modal {
            acknowledge(ctx, component).await?;
        }
        return interact::followup_component_error(ctx, component, "Error", NOT_YOURS).await;
    };
    if !session_alive(ctx, data, &loaded).await? {
        if opens_modal {
            acknowledge(ctx, component).await?;
        }
        return interact::edit_component(ctx, component, render::simple(gone_message(&loaded)))
            .await;
    }
    let repo = ReviewRepository::new(data.db.pool());
    repo.touch(loaded.review.id, loaded.revision.revision)
        .await?;

    match id.action {
        "start" | "done" => show_console(ctx, component, data, loaded.review.id, None).await,
        "replay" => open_replay_modal(ctx, component, loaded.review.id, None).await,
        "media" => open_media_modal(ctx, component, data, &loaded).await,
        "manage" => show_manager(ctx, component, data, loaded.review.id, None).await,
        "evsel" => {
            let selected = selected_value(component).and_then(|v| v.parse().ok());
            show_manager(ctx, component, data, loaded.review.id, selected).await
        }
        "evnote" => {
            let piece = piece_of(&loaded, id.extra);
            open_note_modal(ctx, component, loaded.review.id, piece).await
        }
        "evreplay" => {
            let piece = piece_of(&loaded, id.extra);
            open_replay_modal(ctx, component, loaded.review.id, piece).await
        }
        "evdel" => {
            if let Some(piece) = piece_of(&loaded, id.extra) {
                repo.remove_evidence(piece.id).await?;
            }
            show_manager(ctx, component, data, loaded.review.id, None).await
        }
        "type" => {
            if let Some(tag_type) =
                selected_value(component).filter(|t| REVIEW_TAGS.contains(&t.as_str()))
            {
                repo.set_content(
                    loaded.review.id,
                    loaded.revision.revision,
                    &tag_type,
                    &loaded.revision.reason,
                )
                .await?;
            }
            show_manager(ctx, component, data, loaded.review.id, None).await
        }
        "reason" => open_reason_modal(ctx, component, &loaded).await,
        "discard" => discard(ctx, component, data, &loaded).await,
        "submit" => super::publish::submit(ctx, component, data, loaded).await,
        "save" => super::edit::save(ctx, component, data, loaded).await,
        "withdraw" => super::edit::withdraw_prompt(ctx, component, data, &loaded).await,
        "withdraw_go" => super::edit::withdraw(ctx, component, data, &loaded).await,
        _ => Ok(()),
    }
}

pub async fn handle_modal(ctx: &Context, modal: &ModalInteraction, data: &Data) -> Result<()> {
    let Some(id) = parse_id(&modal.data.custom_id) else {
        return Ok(());
    };
    modal
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let Some(loaded) = authorized(ctx, data, id.review_id, modal.user.id).await? else {
        modal
            .create_followup(
                &ctx.http,
                CreateInteractionResponseFollowup::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(vec![interact::error_container("Error", NOT_YOURS)]),
            )
            .await?;
        return Ok(());
    };
    if !session_alive(ctx, data, &loaded).await? {
        return present_modal(
            ctx,
            modal,
            data,
            render::simple(gone_message(&loaded)),
            None,
        )
        .await;
    }
    let repo = ReviewRepository::new(data.db.pool());
    let review_id = loaded.review.id;
    let revision = loaded.revision.revision;

    let notice = match id.action {
        "replay_m" => {
            let raw = interact::extract_modal_value(&modal.data.components, "replay");
            let note = optional(interact::extract_modal_value(
                &modal.data.components,
                "note",
            ));
            match parse_replay(&raw) {
                None => Some(
                    "That replay could not be read. Paste the /replay command or the replay ID.",
                ),
                Some(replay) => {
                    let existing = piece_of(&loaded, id.extra);
                    if let Some(existing) = existing {
                        repo.remove_evidence(existing.id).await?;
                    }
                    repo.add_replay(
                        review_id,
                        revision,
                        &replay.id,
                        replay.timestamp.as_deref(),
                        note.as_deref(),
                        modal.user.id.get() as i64,
                    )
                    .await?;
                    None
                }
            }
        }
        "note_m" => {
            let note = optional(interact::extract_modal_value(
                &modal.data.components,
                "note",
            ));
            if let Some(piece) = piece_of(&loaded, id.extra) {
                repo.set_note(piece.id, note.as_deref()).await?;
            }
            None
        }
        "reason_m" => {
            let reason = interact::extract_modal_value(&modal.data.components, "reason");
            let reason = reason.trim();
            if !reason.is_empty() {
                repo.set_content(review_id, revision, &loaded.revision.tag_type, reason)
                    .await?;
            }
            None
        }
        "media_m" => return receive_media(ctx, modal, data, &loaded).await,
        _ => return Ok(()),
    };
    repo.touch(review_id, revision).await?;
    let view = console_view(ctx, data, review_id, notice).await?;
    present_modal(ctx, modal, data, view, Some(&loaded.review.player_uuid)).await
}

async fn present_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
    view: Vec<CreateComponent<'static>>,
    face_uuid: Option<&str>,
) -> Result<()> {
    let mut message = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(view);
    if let Some(uuid) = face_uuid {
        message = message.new_attachment(face_attachment(data, uuid).await);
    }
    modal.edit_response(&ctx.http, message).await?;
    Ok(())
}

pub async fn console_view(
    ctx: &Context,
    data: &Data,
    review_id: i64,
    notice: Option<&str>,
) -> Result<Vec<CreateComponent<'static>>> {
    let Some(loaded) = load_working(ctx, data, review_id).await? else {
        return Ok(render::simple(EXPIRED_DRAFT));
    };
    let mode = console_mode(data, &loaded).await?;
    Ok(render::console(&loaded, &mode, notice))
}

async fn console_mode(data: &Data, loaded: &Loaded) -> Result<ConsoleMode> {
    if loaded.review.status == ReviewStatus::Draft {
        return Ok(ConsoleMode::Draft {
            expires_at: loaded.revision.last_activity_at + draft_idle_window(),
        });
    }
    let repo = ReviewRepository::new(data.db.pool());
    let votes = repo.active_votes(loaded.review.id).await?;
    Ok(ConsoleMode::Edit {
        save_clears_votes: votes.iter().any(|v| v.kind == VoteKind::Accept),
        can_withdraw: !repo.ever_voted(loaded.review.id).await?,
    })
}

pub async fn show_console(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review_id: i64,
    notice: Option<&str>,
) -> Result<()> {
    let view = console_view(ctx, data, review_id, notice).await?;
    let review = ReviewRepository::new(data.db.pool()).get(review_id).await?;
    let mut message = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(view);
    if let Some(review) = review {
        message = message.new_attachment(face_attachment(data, &review.player_uuid).await);
    }
    component.edit_response(&ctx.http, message).await?;
    Ok(())
}

async fn show_manager(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review_id: i64,
    selected: Option<i64>,
) -> Result<()> {
    let view = match load_working(ctx, data, review_id).await? {
        Some(loaded) => render::evidence_manager(&loaded, selected),
        None => render::simple(EXPIRED_DRAFT),
    };
    interact::edit_component(ctx, component, view).await
}

async fn open_replay_modal(
    ctx: &Context,
    component: &ComponentInteraction,
    review_id: i64,
    existing: Option<&database::Evidence>,
) -> Result<()> {
    let mut replay = CreateInputText::new(InputTextStyle::Short, "replay")
        .placeholder("/replay 9f2fa87d-ed0b-471b-a2e6-cb42777beec8 #9d303f9d")
        .min_length(1)
        .max_length(200);
    let mut note = CreateInputText::new(InputTextStyle::Short, "note")
        .placeholder("Optional note about this replay")
        .required(false)
        .max_length(NOTE_MAX_CHARS);
    let modal_id = match existing {
        Some(piece) => {
            replay = replay.value(render::replay_command(piece));
            if let Some(existing_note) = &piece.note {
                note = note.value(existing_note.clone());
            }
            custom_id_with("replay_m", review_id, piece.id)
        }
        None => custom_id("replay_m", review_id),
    };
    let modal = CreateModal::new(modal_id, "Add Replay Evidence").components(vec![
        CreateModalComponent::Label(CreateLabel::input_text("Replay Command or ID", replay)),
        CreateModalComponent::Label(CreateLabel::input_text("Note (optional)", note)),
    ]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

async fn open_note_modal(
    ctx: &Context,
    component: &ComponentInteraction,
    review_id: i64,
    piece: Option<&database::Evidence>,
) -> Result<()> {
    let Some(piece) = piece else {
        return Ok(());
    };
    let mut note = CreateInputText::new(InputTextStyle::Short, "note")
        .placeholder("Optional note about this file")
        .required(false)
        .max_length(NOTE_MAX_CHARS);
    if let Some(existing) = &piece.note {
        note = note.value(existing.clone());
    }
    let modal = CreateModal::new(custom_id_with("note_m", review_id, piece.id), "Edit Note")
        .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
            "Note", note,
        ))]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

async fn open_reason_modal(
    ctx: &Context,
    component: &ComponentInteraction,
    loaded: &Loaded,
) -> Result<()> {
    let reason = CreateInputText::new(InputTextStyle::Paragraph, "reason")
        .placeholder("Reason for this tag")
        .value(loaded.revision.reason.clone())
        .max_length(REASON_MAX_CHARS)
        .required(true);
    let modal = CreateModal::new(custom_id("reason_m", loaded.review.id), "Edit Reason")
        .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
            "Reason", reason,
        ))]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

async fn open_media_modal(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    loaded: &Loaded,
) -> Result<()> {
    let room = room_left(data, loaded).await?;
    if room == 0 {
        acknowledge(ctx, component).await?;
        return show_console(
            ctx,
            component,
            data,
            loaded.review.id,
            Some("This review already holds the maximum of 9 files."),
        )
        .await;
    }
    let upload = CreateFileUpload::new("evidence")
        .max_values(room.min(i64::from(MAX_FILES_PER_UPLOAD)) as u8)
        .required(true);
    let note = CreateInputText::new(InputTextStyle::Short, "note")
        .placeholder("Applies to all files in this upload.")
        .required(false)
        .max_length(NOTE_MAX_CHARS);
    let modal =
        CreateModal::new(custom_id("media_m", loaded.review.id), "Add media").components(vec![
            CreateModalComponent::Label(
                CreateLabel::file_upload("Screenshots or clips", upload)
                    .description("png, jpg, gif, webp, mp4, webm or mov, up to 4 files"),
            ),
            CreateModalComponent::Label(CreateLabel::input_text("Note", note)),
        ]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

async fn receive_media(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
    loaded: &Loaded,
) -> Result<()> {
    let note = optional(interact::extract_modal_value(
        &modal.data.components,
        "note",
    ));
    let notice = stage_uploads(ctx, modal, data, loaded, note.as_deref()).await?;
    let repo = ReviewRepository::new(data.db.pool());
    repo.touch(loaded.review.id, loaded.revision.revision)
        .await?;
    let view = console_view(ctx, data, loaded.review.id, notice.as_deref()).await?;
    present_modal(ctx, modal, data, view, Some(&loaded.review.player_uuid)).await
}

async fn stage_uploads(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
    loaded: &Loaded,
    note: Option<&str>,
) -> Result<Option<String>> {
    let uploads: Vec<&Attachment> = modal
        .data
        .components
        .iter()
        .filter_map(|c| match c {
            Component::Label(label) => match &label.component {
                LabelComponent::FileUpload(upload) => Some(upload.values.iter()),
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .filter_map(|id| modal.data.resolved.attachments.get(id))
        .collect();
    let room = room_left(data, loaded).await?;
    if uploads.len() as i64 > room {
        return Ok(Some(format!(
            "That upload has {} files but only {room} more fit on this review.",
            uploads.len()
        )));
    }
    let limit = guild_upload_limit(ctx, modal.guild_id).await;
    let mut accepted = Vec::new();
    for attachment in &uploads {
        let ext = extension(&attachment.filename);
        if !ALLOWED_MEDIA_EXTENSIONS.contains(&ext.as_str()) {
            return Ok(Some(
                "Only images and videos are accepted (png, jpg, gif, webp, mp4, webm, mov).".into(),
            ));
        }
        if u64::from(attachment.size) > limit {
            return Ok(Some(format!(
                "`{}` is larger than the {} this server allows. Compress the clip and try again.",
                attachment.filename,
                format_size(limit)
            )));
        }
        let bytes = match reqwest::get(attachment.url.as_str()).await {
            Ok(response) => response.bytes().await,
            Err(e) => Err(e),
        };
        match bytes {
            Ok(bytes) => accepted.push((
                ext,
                attachment.content_type.as_ref().map(|c| c.to_string()),
                bytes.to_vec(),
            )),
            Err(e) => {
                tracing::warn!(
                    "Failed to download review upload {}: {e}",
                    attachment.filename
                );
                return Ok(Some(format!(
                    "`{}` could not be downloaded, so nothing was added. Try again.",
                    attachment.filename
                )));
            }
        }
    }
    let repo = ReviewRepository::new(data.db.pool());
    let mut number = next_media_number(loaded);
    for (ext, content_type, bytes) in accepted {
        let filename = format!("{}_{number}.{ext}", loaded.username);
        repo.add_staged_media(
            loaded.review.id,
            loaded.revision.revision,
            &filename,
            content_type.as_deref(),
            &bytes,
            note,
            modal.user.id.get() as i64,
        )
        .await?;
        number += 1;
    }
    Ok(None)
}

fn next_media_number(loaded: &Loaded) -> usize {
    loaded
        .evidence
        .iter()
        .filter_map(|e| e.filename.as_deref())
        .filter_map(|f| {
            let stem = f.rsplit_once('.').map_or(f, |(stem, _)| stem);
            stem.rsplit_once('_')?.1.parse::<usize>().ok()
        })
        .max()
        .unwrap_or(0)
        + 1
}

async fn room_left(data: &Data, loaded: &Loaded) -> Result<i64> {
    let used = ReviewRepository::new(data.db.pool())
        .media_count(loaded.review.id, loaded.revision.revision)
        .await?;
    Ok((MAX_MEDIA - used).max(0))
}

async fn discard(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    loaded: &Loaded,
) -> Result<()> {
    let repo = ReviewRepository::new(data.db.pool());
    if loaded.review.status == ReviewStatus::Draft {
        repo.discard_draft(loaded.review.id, loaded.review.submitter_id)
            .await?;
        return interact::edit_component(
            ctx,
            component,
            render::simple(format!(
                "Your draft review of `{}` was discarded.",
                loaded.username
            )),
        )
        .await;
    }
    repo.cancel_edit(loaded.review.id).await?;
    super::publish::rerender_post(ctx, data, loaded.review.id).await;
    interact::edit_component(
        ctx,
        component,
        render::simple(format!(
            "Your edit was cancelled. The review of `{}` and its votes are unchanged.",
            loaded.username
        )),
    )
    .await
}

async fn authorized(
    ctx: &Context,
    data: &Data,
    review_id: i64,
    user: UserId,
) -> Result<Option<Loaded>> {
    let repo = ReviewRepository::new(data.db.pool());
    let Some(review) = repo.get(review_id).await? else {
        return Ok(None);
    };
    if review.submitter_id as u64 != user.get() {
        return Ok(None);
    }
    match load_working(ctx, data, review_id).await? {
        Some(loaded) => Ok(Some(loaded)),
        None => Ok(super::load_live(ctx, data, review_id).await?),
    }
}

async fn session_alive(ctx: &Context, data: &Data, loaded: &Loaded) -> Result<bool> {
    let repo = ReviewRepository::new(data.db.pool());
    match loaded.review.status {
        ReviewStatus::Draft => Ok(!repo
            .expire_idle_draft(loaded.review.id, draft_idle_window())
            .await?),
        ReviewStatus::Open if loaded.revision.published_at.is_none() => {
            let lapsed = repo
                .cancel_idle_edit(loaded.review.id, edit_idle_window())
                .await?;
            if lapsed {
                deadline::after_edit_lapse(ctx, data, loaded.review.id);
            }
            Ok(!lapsed)
        }
        _ => Ok(false),
    }
}

fn gone_message(loaded: &Loaded) -> String {
    match loaded.review.status {
        ReviewStatus::Draft | ReviewStatus::Expired => EXPIRED_DRAFT.to_string(),
        ReviewStatus::Open => format!(
            "Your edit was cancelled. The review of `{}` and its votes are unchanged.",
            loaded.username
        ),
        ReviewStatus::Voided => "This review was voided.".to_string(),
        ReviewStatus::Accepted | ReviewStatus::Rejected => {
            "This review is already resolved.".to_string()
        }
    }
}

async fn acknowledge(ctx: &Context, component: &ComponentInteraction) -> Result<()> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    Ok(())
}

fn piece_of<'a>(loaded: &'a Loaded, extra: Option<&str>) -> Option<&'a database::Evidence> {
    let id: i64 = extra?.parse().ok()?;
    loaded.evidence.iter().find(|e| e.id == id)
}

fn selected_value(component: &ComponentInteraction) -> Option<String> {
    match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().map(|v| v.to_string())
        }
        _ => None,
    }
}

fn optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub async fn guild_upload_limit(ctx: &Context, guild_id: Option<GuildId>) -> u64 {
    const MIB: u64 = 1024 * 1024;
    let Some(guild_id) = guild_id else {
        return 10 * MIB;
    };
    match ctx.http.get_guild(guild_id).await.map(|g| g.premium_tier) {
        Ok(PremiumTier::Tier2) => 50 * MIB,
        Ok(PremiumTier::Tier3) => 100 * MIB,
        Ok(_) => 10 * MIB,
        Err(e) => {
            tracing::warn!("Could not resolve guild upload limit, assuming 10 MiB: {e}");
            10 * MIB
        }
    }
}

fn format_size(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}
