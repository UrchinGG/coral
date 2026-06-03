use std::collections::HashMap;

use anyhow::Result;
use blacklist::lookup as lookup_tag;
use database::BlacklistRepository;
use serenity::all::*;

use super::channel::{COLOR_DANGER, format_tag_block};
use super::reviews;
use super::tag::get_rank;
use crate::framework::{AccessRank, Data};
use crate::utils::{format_uuid_dashed, sanitize_reason, separator, text};
use coral_redis::BlacklistEvent;

fn extract_uuid_from_title(title: &str) -> Option<String> {
    let last = title.rsplit('|').next()?.trim();
    let uuid = last.replace('-', "");
    (uuid.len() == 32 && uuid.chars().all(|c| c.is_ascii_hexdigit())).then_some(uuid)
}

pub fn thread_index_insert(data: &Data, name: &str, thread_id: ThreadId, parent_id: ChannelId) {
    if data.evidence_forum_id != Some(parent_id) {
        return;
    }
    let Some(uuid) = extract_uuid_from_title(name) else {
        return;
    };
    data.evidence_threads
        .write()
        .unwrap()
        .insert(uuid, thread_id);
}

pub fn thread_index_remove(data: &Data, thread_id: ThreadId) {
    data.evidence_threads
        .write()
        .unwrap()
        .retain(|_, id| *id != thread_id);
}

pub async fn populate_thread_index(ctx: &Context, data: &Data) {
    let Some(forum_id) = data.evidence_forum_id else {
        return;
    };
    let Some(guild_id) = data.home_guild_id else {
        return;
    };

    let mut found: HashMap<String, ThreadId> = HashMap::new();

    match ctx.http.get_guild_active_threads(guild_id).await {
        Ok(active) => {
            for t in &active.threads {
                if t.parent_id == forum_id {
                    if let Some(uuid) = extract_uuid_from_title(&t.base.name) {
                        found.insert(uuid, t.id);
                    }
                }
            }
        }
        Err(e) => tracing::warn!("evidence index: failed to list active threads: {e}"),
    }

    let mut before: Option<Timestamp> = None;
    loop {
        match ctx
            .http
            .get_channel_archived_public_threads(forum_id, before, Some(100))
            .await
        {
            Ok(batch) => {
                for t in &batch.threads {
                    if let Some(uuid) = extract_uuid_from_title(&t.base.name) {
                        found.insert(uuid, t.id);
                    }
                }
                let next_before = batch
                    .threads
                    .last()
                    .and_then(|t| t.thread_metadata.archive_timestamp);
                if !batch.has_more || next_before.is_none() {
                    break;
                }
                before = next_before;
            }
            Err(e) => {
                tracing::warn!("evidence index: failed to page archived threads: {e}");
                break;
            }
        }
    }

    let count = found.len();
    *data.evidence_threads.write().unwrap() = found;
    tracing::info!("evidence thread index populated: {count} threads");
}

pub fn evidence_thread_url(data: &Data, uuid: &str) -> Option<String> {
    let thread_id = data.evidence_thread_for(uuid)?;
    let guild_id = data.home_guild_id?;
    Some(format!(
        "https://discord.com/channels/{guild_id}/{thread_id}"
    ))
}
const ALLOWED_MEDIA_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm", "mov"];
const MAX_EVIDENCE_MEDIA: u8 = 10;

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("confirm")
        .description("Create an evidence post and confirm a cheater tag")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "player", "Player name or UUID")
                .required(true),
        )
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;

    let discord_id = command.user.id.get();
    let rank = get_rank(data, discord_id).await?;
    if rank < AccessRank::Trusted {
        return crate::interact::send_deferred_error(
            ctx,
            command,
            "Error",
            "Only trusted users and above can use this command",
        )
        .await;
    }

    let player_name = command
        .data
        .options()
        .iter()
        .find_map(|o| match (&*o.name, &o.value) {
            ("player", ResolvedValue::String(s)) => Some(*s),
            _ => None,
        })
        .unwrap_or("");

    let player_info = match data.api.resolve(player_name).await {
        Ok(info) => info,
        Err(_) => {
            return crate::interact::send_deferred_error(ctx, command, "Error", "Player not found")
                .await;
        }
    };

    let repo = BlacklistRepository::new(data.db.pool());
    let tags = repo.get_active_tags(&player_info.uuid).await?;

    if tags
        .iter()
        .any(|t| t.tag_type.as_deref() == Some("confirmed_cheater"))
    {
        return crate::interact::send_deferred_error(
            ctx,
            command,
            "Error",
            "Player is already confirmed",
        )
        .await;
    }

    let Some(tag) = tags.iter().find(|t| {
        matches!(
            t.tag_type.as_deref(),
            Some("closet_cheater" | "blatant_cheater")
        )
    }) else {
        return crate::interact::send_deferred_error(
            ctx,
            command,
            "Error",
            "Player must have a closet cheater or blatant cheater tag",
        )
        .await;
    };

    if let Some(thread_url) = evidence_thread_url(data, &player_info.uuid) {
        let emote = lookup_tag("confirmed_cheater")
            .map(|d| d.emote)
            .unwrap_or("");
        command
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(vec![CreateComponent::Container(CreateContainer::new(
                        vec![CreateContainerComponent::TextDisplay(
                            CreateTextDisplay::new(format!(
                                "## {} Evidence Already Exists\nPlayer: `{}`\nThread: {}",
                                emote, player_info.username, thread_url
                            )),
                        )],
                    ))]),
            )
            .await?;
        return Ok(());
    }

    if rank < AccessRank::Helper {
        return run_member_confirm(ctx, command, data, discord_id, &player_info, tag).await;
    }

    let reason = tag.reason.clone().unwrap_or_default();
    run_staff_confirm(ctx, command, data, &player_info, &reason).await
}

async fn run_member_confirm(
    ctx: &Context,
    command: &CommandInteraction,
    data: &Data,
    discord_id: u64,
    player_info: &crate::api::ResolveResponse,
    tag: &database::PlayerEvent,
) -> Result<()> {
    let reason = tag.reason.as_deref().unwrap_or("");
    let thread_id = reviews::create_submission(
        ctx,
        data,
        discord_id,
        &player_info.username,
        &player_info.uuid,
        tag.tag_type.as_deref().unwrap_or(""),
        reason,
    )
    .await?;

    reviews::spawn_submission_timeout(ctx.clone(), thread_id);

    let emote = lookup_tag("confirmed_cheater")
        .map(|d| d.emote)
        .unwrap_or("");
    command.edit_response(
        &ctx.http,
        EditInteractionResponse::new()
            .flags(MessageFlags::IS_COMPONENTS_V2)
            .components(vec![CreateComponent::Container(CreateContainer::new(
                vec![CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
                    "## {} Review Submitted\nPlayer: `{}`\nThread: <#{}>\n-# Add evidence to the thread to proceed",
                    emote, player_info.username, thread_id.get()
                )))],
            ))]),
    ).await?;
    Ok(())
}

async fn run_staff_confirm(
    ctx: &Context,
    command: &CommandInteraction,
    data: &Data,
    player_info: &crate::api::ResolveResponse,
    reason: &str,
) -> Result<()> {
    let Some(forum_id) = data.evidence_forum_id else {
        return crate::interact::send_deferred_error(
            ctx,
            command,
            "Error",
            "Evidence forum channel not configured",
        )
        .await;
    };

    let thread_title = format!(
        "{} | {}",
        player_info.username,
        format_uuid_dashed(&player_info.uuid)
    );
    let message_content = build_evidence_message(
        &player_info.username,
        &player_info.uuid,
        reason,
        &[],
        None,
        &HashMap::new(),
    );

    let face = face_attachment(data, &player_info.uuid).await;
    let thread = forum_id
        .create_forum_post(
            &ctx.http,
            CreateForumPost::new(
                thread_title.clone(),
                CreateMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(message_content)
                    .add_file(face),
            ),
        )
        .await?;

    thread_index_insert(
        data,
        &thread_title,
        ThreadId::new(thread.id.get()),
        forum_id,
    );

    let emote = lookup_tag("confirmed_cheater")
        .map(|d| d.emote)
        .unwrap_or("");
    command
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(CreateContainer::new(
                    vec![CreateContainerComponent::TextDisplay(
                        CreateTextDisplay::new(format!(
                            "## {} Evidence Post Created\nPlayer: `{}`\nThread: <#{}>",
                            emote,
                            player_info.username,
                            thread.id.get()
                        )),
                    )],
                ))]),
        )
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
struct EvidenceItem {
    filename: String,
}

struct EvidenceState {
    username: String,
    uuid: String,
    reason: String,
    evidence: Vec<EvidenceItem>,
    review_url: Option<String>,
}

const FACE_FILENAME: &str = "face.png";
const FACE_SIZE: u32 = 128;

fn face_section(content: String) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            content,
        ))],
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
            format!("attachment://{FACE_FILENAME}"),
        ))),
    ))
}

async fn face_attachment(data: &Data, uuid: &str) -> CreateAttachment<'static> {
    let png = data
        .skin_provider
        .fetch_face(uuid, FACE_SIZE)
        .await
        .unwrap_or_else(default_face_png);
    CreateAttachment::bytes(png, FACE_FILENAME)
}

fn default_face_png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(FACE_SIZE, FACE_SIZE, image::Rgba([0, 0, 0, 0]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn gallery_url_map(message: &Message) -> HashMap<String, String> {
    let Some(container) = message.components.iter().find_map(|c| match c {
        Component::Container(c) => Some(c),
        _ => None,
    }) else {
        return HashMap::new();
    };

    let mut map = HashMap::new();
    for part in &*container.components {
        if let ContainerComponent::MediaGallery(gallery) = part {
            for item in &*gallery.items {
                let url = item.media.url.to_string();
                if !url.starts_with("attachment://") {
                    let filename = url
                        .rsplit('/')
                        .next()
                        .unwrap_or("unknown.png")
                        .split('?')
                        .next()
                        .unwrap_or("unknown.png")
                        .to_string();
                    map.insert(filename, url);
                }
            }
        }
    }
    map
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

fn url_extension(url: &str) -> &str {
    url.rsplit('/')
        .next()
        .unwrap_or("png")
        .split('?')
        .next()
        .unwrap_or("png")
        .rsplit('.')
        .next()
        .unwrap_or("png")
}

fn build_evidence_message(
    username: &str,
    uuid: &str,
    reason: &str,
    evidence: &[EvidenceItem],
    review_thread_url: Option<&str>,
    gallery_urls: &HashMap<String, String>,
) -> Vec<CreateComponent<'static>> {
    let dashed_uuid = format_uuid_dashed(uuid);

    let block = format_tag_block(
        "confirmed_cheater",
        &sanitize_reason(reason),
        "",
        None,
        None,
        false,
    );

    let mut uuid_footer = format!("-# UUID: {dashed_uuid}");
    if let Some(url) = review_thread_url {
        uuid_footer.push_str(&format!(" · [Review]({url})"));
    }

    let mut parts: Vec<CreateContainerComponent<'static>> = Vec::new();
    parts.push(text(format!("## Evidence — `{username}`")));
    if evidence.is_empty() {
        parts.push(text("-# No evidence added yet"));
    } else {
        let items: Vec<CreateMediaGalleryItem<'static>> = evidence
            .iter()
            .map(|e| {
                let url = gallery_urls
                    .get(&e.filename)
                    .cloned()
                    .unwrap_or_else(|| format!("attachment://{}", e.filename));
                CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url))
            })
            .collect();
        parts.push(CreateContainerComponent::MediaGallery(
            CreateMediaGallery::new(items),
        ));
    }
    parts.push(face_section(format!("{block}\n{uuid_footer}")));
    parts.push(separator());

    if !evidence.is_empty() {
        let options: Vec<CreateSelectMenuOption<'static>> = evidence
            .iter()
            .enumerate()
            .map(|(i, e)| CreateSelectMenuOption::new(e.filename.clone(), i.to_string()))
            .collect();
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::SelectMenu(
                CreateSelectMenu::new(
                    "evidence_remove",
                    CreateSelectMenuKind::String {
                        options: options.into(),
                    },
                )
                .placeholder("Remove evidence..."),
            ),
        ));
    }

    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(
            vec![
                CreateButton::new("evidence_add_media")
                    .label("Add Media")
                    .style(ButtonStyle::Primary),
                CreateButton::new("evidence_archive")
                    .label("Archive")
                    .style(ButtonStyle::Danger),
            ]
            .into(),
        ),
    ));

    vec![CreateComponent::Container(
        CreateContainer::new(parts).accent_color(COLOR_DANGER),
    )]
}

fn build_archived_evidence_message(
    state: &EvidenceState,
    gallery_urls: &HashMap<String, String>,
) -> Vec<CreateComponent<'static>> {
    let dashed_uuid = format_uuid_dashed(&state.uuid);

    let removed_line = "> -# **\\- Tag removed**".to_string();
    let block = format_tag_block(
        "confirmed_cheater",
        &sanitize_reason(&state.reason),
        "",
        Some(&removed_line),
        None,
        true,
    );

    let mut uuid_footer = format!("-# UUID: {dashed_uuid}");
    if let Some(url) = &state.review_url {
        uuid_footer.push_str(&format!(" · [Review]({url})"));
    }

    let mut parts: Vec<CreateContainerComponent<'static>> = Vec::new();
    parts.push(text(format!(
        "## Evidence — `{}` (Archived)",
        state.username
    )));
    if !state.evidence.is_empty() {
        let items: Vec<CreateMediaGalleryItem<'static>> = state
            .evidence
            .iter()
            .map(|e| {
                let url = gallery_urls
                    .get(&e.filename)
                    .cloned()
                    .unwrap_or_else(|| format!("attachment://{}", e.filename));
                CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url))
            })
            .collect();
        parts.push(CreateContainerComponent::MediaGallery(
            CreateMediaGallery::new(items),
        ));
    }
    parts.push(face_section(format!("{block}\n{uuid_footer}")));
    parts.push(separator());

    vec![CreateComponent::Container(
        CreateContainer::new(parts).accent_color(COLOR_DANGER),
    )]
}

fn parse_state_from_message(message: &Message) -> Option<EvidenceState> {
    let container = message.components.iter().find_map(|c| match c {
        Component::Container(c) => Some(c),
        _ => None,
    })?;

    let mut state = EvidenceState {
        username: String::new(),
        uuid: String::new(),
        reason: String::new(),
        evidence: Vec::new(),
        review_url: None,
    };

    for part in &container.components {
        match part {
            ContainerComponent::TextDisplay(td) => {
                ingest_text(&mut state, td.content.as_deref().unwrap_or(""));
            }
            ContainerComponent::Section(section) => {
                for sc in &*section.components {
                    if let SectionComponent::TextDisplay(td) = sc {
                        ingest_text(&mut state, td.content.as_deref().unwrap_or(""));
                    }
                }
            }
            ContainerComponent::MediaGallery(gallery) => {
                for item in &*gallery.items {
                    let url = item
                        .media
                        .proxy_url
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| item.media.url.to_string());
                    if !url.is_empty() {
                        let filename = url.rsplit('/').next().unwrap_or("evidence.png");
                        let filename = filename.split('?').next().unwrap_or(filename);
                        state.evidence.push(EvidenceItem {
                            filename: filename.to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if state.uuid.is_empty() {
        return None;
    }
    Some(state)
}

fn ingest_text(state: &mut EvidenceState, content: &str) {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("## ").and_then(extract_evidence_name) {
            state.username = name;
        } else if let Some(rest) = trimmed.strip_prefix("-# UUID: ") {
            state.uuid = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .replace('-', "");
            if let Some(url) = rest.split("[Review](").nth(1) {
                state.review_url = url.split(')').next().map(|s| s.to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("UUID: `") {
            state.uuid = rest.trim_end_matches('`').replace('-', "");
        } else if state.reason.is_empty() {
            if let Some(rest) = trimmed.strip_prefix("> ") {
                if !rest.starts_with("-#") {
                    state.reason = rest.to_string();
                }
            }
        }
    }
}

fn extract_evidence_name(header: &str) -> Option<String> {
    let after = header.split(" — `").nth(1)?;
    Some(
        after
            .trim_end_matches('`')
            .trim_end_matches(" (Archived)")
            .to_string(),
    )
}

async fn try_convert_to_confirmed(data: &Data, state: &EvidenceState, actor_id: u64) -> Result<()> {
    let repo = BlacklistRepository::new(data.db.pool());
    let tags = repo.get_active_tags(&state.uuid).await?;
    if tags
        .iter()
        .any(|t| t.tag_type.as_deref() == Some("confirmed_cheater"))
    {
        return Ok(());
    }
    let Some(tag) = tags
        .iter()
        .find(|t| t.tag_type.as_deref() != Some("confirmed_cheater"))
    else {
        return Ok(());
    };
    let old_tag_type = tag.tag_type.clone().unwrap_or_default();
    let old_reason = tag.reason.clone().unwrap_or_default();
    let old_tag_id = tag.id;
    let blocking = blacklist::priority_lane_excluding("confirmed_cheater", &old_tag_type);
    let outcome = repo
        .overwrite_event(
            &state.uuid,
            &old_tag_type,
            "confirmed_cheater",
            &old_reason,
            tag.hide_username.unwrap_or(false),
            None,
            None,
            Some(actor_id as i64),
            &blocking,
            Some(tag.id),
        )
        .await?;
    if let database::OverwriteOutcome::Inserted { new, .. } = outcome {
        data.event_publisher
            .publish(&BlacklistEvent::TagOverwritten {
                uuid: state.uuid.clone(),
                old_tag_id,
                old_tag_type,
                old_reason,
                new_tag_id: new.id,
                overwritten_by: actor_id as i64,
            })
            .await;
    }
    Ok(())
}

pub async fn handle_add_media(
    ctx: &Context,
    component: &ComponentInteraction,
    _data: &Data,
) -> Result<()> {
    let upload = CreateFileUpload::new("evidence")
        .max_values(MAX_EVIDENCE_MEDIA)
        .required(true);
    let modal = CreateModal::new("evidence_media_modal", "Upload Evidence").components(vec![
        CreateModalComponent::Label(CreateLabel::file_upload(
            "Evidence screenshots or clips",
            upload,
        )),
    ]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_media_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    modal.defer_ephemeral(&ctx.http).await?;

    let attachment_ids: Vec<AttachmentId> = modal
        .data
        .components
        .iter()
        .filter_map(|c| match c {
            Component::Label(label) => match &label.component {
                LabelComponent::FileUpload(fu) => Some(fu.values.iter().copied()),
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .collect();

    if attachment_ids.is_empty() {
        let _ = modal.delete_response(&ctx.http).await;
        return Ok(());
    }

    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().content("Downloading files..."),
        )
        .await?;

    let channel_id = modal.channel_id;
    let builder_msg_id = MessageId::new(channel_id.get());
    let Ok(builder_msg) = ctx
        .http
        .get_message(channel_id.into(), builder_msg_id)
        .await
    else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not find the evidence message"),
            )
            .await?;
        return Ok(());
    };

    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not parse evidence state"),
            )
            .await?;
        return Ok(());
    };

    let existing_count = state.evidence.len();
    let mut files = Vec::new();
    let mut rejected = 0usize;

    for (i, att_id) in attachment_ids.iter().enumerate() {
        let Some(attachment) = modal.data.resolved.attachments.get(att_id) else {
            continue;
        };
        let ext = url_extension(&attachment.filename).to_ascii_lowercase();
        if !ALLOWED_MEDIA_EXTENSIONS.contains(&ext.as_str()) {
            rejected += 1;
            continue;
        }
        let filename = format!("{}_{}.{}", state.username, existing_count + i + 1, ext);
        match CreateAttachment::url(&ctx.http, attachment.url.as_str(), filename.clone()).await {
            Ok(file) => {
                files.push(file);
                state.evidence.push(EvidenceItem { filename });
            }
            Err(e) => {
                tracing::warn!("Failed to download attachment: {e}");
                rejected += 1;
            }
        }
    }

    if files.is_empty() && rejected > 0 {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content(
                    "Only images and videos are accepted (png, jpg, gif, webp, mp4, webm, mov)",
                ),
            )
            .await?;
        return Ok(());
    }

    let urls = gallery_url_map(&builder_msg);
    let components = build_evidence_message(
        &state.username,
        &state.uuid,
        &state.reason,
        &state.evidence,
        state.review_url.as_deref(),
        &urls,
    );

    let face = face_attachment(data, &state.uuid).await;
    let mut attachments = EditAttachments::new();
    for url in urls.values() {
        if let Some(id) = attachment_id_from_cdn_url(url) {
            attachments = attachments.keep(id);
        }
    }
    attachments = attachments.add(face.clone());
    for f in files.iter().cloned() {
        attachments = attachments.add(f);
    }
    let mut all_files = files.clone();
    all_files.push(face);

    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().content("Uploading evidence..."),
        )
        .await?;

    let edit = EditMessage::new()
        .content("")
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components)
        .attachments(attachments);

    match ctx
        .http
        .edit_message(channel_id.into(), builder_msg.id, &edit, all_files)
        .await
    {
        Ok(_) => {
            if existing_count == 0 {
                try_convert_to_confirmed(data, &state, modal.user.id.get()).await?;
            }
            let _ = modal.delete_response(&ctx.http).await;
        }
        Err(e) => {
            let msg = if e.to_string().contains("too large") || e.to_string().contains("413") {
                "File too large. Try compressing or using a smaller file."
            } else {
                "Failed to upload evidence. Please try again."
            };
            modal
                .edit_response(&ctx.http, EditInteractionResponse::new().content(msg))
                .await?;
        }
    }
    Ok(())
}

pub async fn handle_remove(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let discord_id = component.user.id.get();
    let rank = get_rank(data, discord_id).await?;
    if rank < AccessRank::Helper {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only helpers and above can remove evidence",
        )
        .await;
    }

    let idx: usize = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().and_then(|v| v.parse().ok()).unwrap_or(0)
        }
        _ => return Ok(()),
    };

    let channel_id = component.channel_id;
    let builder_msg_id = MessageId::new(channel_id.get());
    let Ok(builder_msg) = ctx
        .http
        .get_message(channel_id.into(), builder_msg_id)
        .await
    else {
        return Ok(());
    };

    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Could not parse evidence state",
        )
        .await;
    };

    if idx >= state.evidence.len() {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Invalid evidence index",
        )
        .await;
    }

    let removed_filename = state.evidence.remove(idx).filename;
    let mut urls = gallery_url_map(&builder_msg);
    urls.remove(&removed_filename);

    let components = build_evidence_message(
        &state.username,
        &state.uuid,
        &state.reason,
        &state.evidence,
        state.review_url.as_deref(),
        &urls,
    );

    let face = face_attachment(data, &state.uuid).await;
    let mut attachments = EditAttachments::new();
    for url in urls.values() {
        if let Some(id) = attachment_id_from_cdn_url(url) {
            attachments = attachments.keep(id);
        }
    }
    attachments = attachments.add(face.clone());

    let edit = EditMessage::new()
        .content("")
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components)
        .attachments(attachments);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    ctx.http
        .edit_message(channel_id.into(), builder_msg_id, &edit, vec![face])
        .await?;
    Ok(())
}

async fn remove_confirmed_tag(
    repo: &BlacklistRepository<'_>,
    state: &EvidenceState,
    actor: i64,
) -> Result<()> {
    repo.remove_event(&state.uuid, "confirmed_cheater", Some(actor))
        .await?;
    Ok(())
}

pub async fn archive_evidence_for_uuid(ctx: &Context, data: &Data, uuid: &str) -> Result<()> {
    let Some(thread_id) = data.evidence_thread_for(uuid) else {
        return Ok(());
    };

    let channel_id: GenericChannelId = thread_id.into();
    let builder_msg_id = MessageId::new(thread_id.get());

    let Ok(builder_msg) = ctx.http.get_message(channel_id, builder_msg_id).await else {
        return Ok(());
    };
    let Some(state) = parse_state_from_message(&builder_msg) else {
        return Ok(());
    };

    let urls = gallery_url_map(&builder_msg);
    let face = face_attachment(data, &state.uuid).await;
    let mut attachments = EditAttachments::new();
    for url in urls.values() {
        if let Some(id) = attachment_id_from_cdn_url(url) {
            attachments = attachments.keep(id);
        }
    }
    attachments = attachments.add(face.clone());
    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(build_archived_evidence_message(&state, &urls))
        .attachments(attachments);

    let _ = ctx
        .http
        .edit_message(channel_id, builder_msg_id, &edit, vec![face])
        .await;
    let _ = thread_id
        .edit(&ctx.http, EditThread::new().archived(true).locked(true))
        .await;
    Ok(())
}

pub async fn handle_archive(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let discord_id = component.user.id.get();
    let rank = get_rank(data, discord_id).await?;
    if rank < AccessRank::Helper {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only helpers and above can archive evidence",
        )
        .await;
    }

    let Some(state) = parse_state_from_message(&*component.message) else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Could not parse evidence state",
        )
        .await;
    };

    let repo = BlacklistRepository::new(data.db.pool());
    remove_confirmed_tag(&repo, &state, discord_id as i64).await?;

    let urls = gallery_url_map(&*component.message);
    let face = face_attachment(data, &state.uuid).await;
    let mut attachments = EditAttachments::new();
    for url in urls.values() {
        if let Some(id) = attachment_id_from_cdn_url(url) {
            attachments = attachments.keep(id);
        }
    }
    attachments = attachments.add(face.clone());

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(build_archived_evidence_message(&state, &urls))
        .attachments(attachments);
    let _ = ctx
        .http
        .edit_message(
            component.channel_id.into(),
            component.message.id,
            &edit,
            vec![face],
        )
        .await;

    let thread_id = ThreadId::new(component.channel_id.get());
    let _ = thread_id
        .edit(&ctx.http, EditThread::new().archived(true).locked(true))
        .await;
    Ok(())
}

pub async fn create_evidence_from_review(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    username: &str,
    reason: &str,
    media_urls: &[String],
    review_thread_url: Option<&str>,
) -> Result<()> {
    let Some(forum_id) = data.evidence_forum_id else {
        anyhow::bail!("Evidence forum channel not configured");
    };

    let mut evidence: Vec<EvidenceItem> = Vec::new();
    let mut files: Vec<CreateAttachment<'static>> = Vec::new();
    for (i, url) in media_urls.iter().enumerate() {
        let ext = url_extension(url);
        let filename = format!("{}_{}.{}", username, i + 1, ext);
        if let Ok(att) = CreateAttachment::url(&ctx.http, url, filename.clone()).await {
            evidence.push(EvidenceItem { filename });
            files.push(att);
        }
    }

    let thread_title = format!("{} | {}", username, format_uuid_dashed(uuid));
    let no_urls = HashMap::new();
    let initial_components =
        build_evidence_message(username, uuid, reason, &[], review_thread_url, &no_urls);

    let initial_face = face_attachment(data, uuid).await;
    let thread = forum_id
        .create_forum_post(
            &ctx.http,
            CreateForumPost::new(
                thread_title.clone(),
                CreateMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(initial_components)
                    .add_file(initial_face),
            ),
        )
        .await?;

    if !files.is_empty() {
        let builder_msg_id = MessageId::new(thread.id.get());
        let channel_id: GenericChannelId = thread.id.into();

        let face = face_attachment(data, uuid).await;
        let mut att = EditAttachments::new();
        for f in &files {
            att = att.add(f.clone());
        }
        att = att.add(face.clone());

        let mut all_files = files.clone();
        all_files.push(face);

        let edit = EditMessage::new()
            .content("")
            .flags(MessageFlags::IS_COMPONENTS_V2)
            .components(build_evidence_message(
                username,
                uuid,
                reason,
                &evidence,
                review_thread_url,
                &no_urls,
            ))
            .attachments(att);

        ctx.http
            .edit_message(channel_id, builder_msg_id, &edit, all_files)
            .await?;
    }

    thread_index_insert(
        data,
        &thread_title,
        ThreadId::new(thread.id.get()),
        forum_id,
    );

    Ok(())
}
