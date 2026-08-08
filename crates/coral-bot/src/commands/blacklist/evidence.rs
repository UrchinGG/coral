use std::collections::HashMap;

use anyhow::Result;
use blacklist::{EMOTE_EVIDENCE, EMOTE_NO_EVIDENCE, lookup as lookup_tag};
use database::BlacklistRepository;
use serenity::all::*;

use super::channel::{format_added_line, format_reviewed_line, format_tag_block};
use super::reviews;
use super::tag::{get_rank, get_rank_and_member};
use crate::framework::{AccessRank, Data, EvidenceThread};
use crate::utils::{format_uuid_dashed, sanitize_reason, separator, text, unsanitize_reason};
use coral_redis::BlacklistEvent;

fn extract_uuid_from_title(title: &str) -> Option<String> {
    title
        .split(|c: char| !c.is_ascii_hexdigit() && c != '-')
        .map(|token| token.replace('-', ""))
        .find(|uuid| uuid.len() == 32)
        .map(|uuid| uuid.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{
        EvidenceItem, display_name_from_title, extract_uuid_from_title, next_evidence_index,
        thread_matches,
    };

    fn items(names: &[&str]) -> Vec<EvidenceItem> {
        names
            .iter()
            .map(|n| EvidenceItem {
                filename: n.to_string(),
            })
            .collect()
    }

    #[test]
    fn display_name_coral_title() {
        assert_eq!(
            display_name_from_title("Scutes | 069a79f4-44e9-4726-a5be-fca90e38aaf5"),
            "Scutes"
        );
    }

    #[test]
    fn display_name_urchin_title() {
        assert_eq!(
            display_name_from_title("Scutes | UUID: 069a79f4-44e9-4726-a5be-fca90e38aaf5"),
            "Scutes"
        );
    }

    #[test]
    fn display_name_bare_uuid_title() {
        assert_eq!(
            display_name_from_title("069a79f4-44e9-4726-a5be-fca90e38aaf5"),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        );
    }

    #[test]
    fn search_matches_name_case_insensitively() {
        assert!(thread_matches("Scutes", "069a79f4", "scut"));
        assert!(thread_matches("Scutes", "069a79f4", "SCUTES"));
        assert!(!thread_matches("Scutes", "069a79f4", "notaplayer"));
    }

    #[test]
    fn search_matches_uuid_with_or_without_dashes() {
        let uuid = "069a79f444e94726a5befca90e38aaf5";
        assert!(thread_matches("Scutes", uuid, uuid));
        assert!(thread_matches(
            "Scutes",
            uuid,
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        ));
        assert!(thread_matches("Scutes", uuid, "069a79f4"));
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(!thread_matches("Scutes", "069a79f4", "   "));
    }

    #[test]
    fn next_index_follows_the_highest_in_use() {
        assert_eq!(next_evidence_index(&items(&[])), 1);
        assert_eq!(
            next_evidence_index(&items(&["Scutes_1.png", "Scutes_2.png"])),
            3
        );
    }

    #[test]
    fn next_index_does_not_reuse_a_removed_slot() {
        // Scutes_1 was removed; the next upload must not become Scutes_2 again.
        assert_eq!(next_evidence_index(&items(&["Scutes_2.png"])), 3);
    }

    #[test]
    fn coral_title() {
        assert_eq!(
            extract_uuid_from_title("Scutes | 069a79f4-44e9-4726-a5be-fca90e38aaf5").as_deref(),
            Some("069a79f444e94726a5befca90e38aaf5")
        );
    }

    #[test]
    fn urchin_bare_uuid_title() {
        assert_eq!(
            extract_uuid_from_title("069a79f4-44e9-4726-a5be-fca90e38aaf5").as_deref(),
            Some("069a79f444e94726a5befca90e38aaf5")
        );
    }

    #[test]
    fn urchin_renamed_title() {
        assert_eq!(
            extract_uuid_from_title("Scutes | UUID: 069a79f4-44e9-4726-a5be-fca90e38aaf5")
                .as_deref(),
            Some("069a79f444e94726a5befca90e38aaf5")
        );
    }

    #[test]
    fn uppercase_uuid_is_lowercased() {
        assert_eq!(
            extract_uuid_from_title("069A79F4-44E9-4726-A5BE-FCA90E38AAF5").as_deref(),
            Some("069a79f444e94726a5befca90e38aaf5")
        );
    }

    #[test]
    fn title_without_uuid() {
        assert_eq!(extract_uuid_from_title("general discussion"), None);
        assert_eq!(extract_uuid_from_title("Scutes | deadbeef"), None);
    }
}

fn display_name_from_title(title: &str) -> &str {
    title.split_once(" | ").map(|(n, _)| n).unwrap_or(title)
}

pub fn thread_index_insert(data: &Data, name: &str, thread_id: ThreadId, parent_id: ChannelId) {
    if data.evidence_forum_id != Some(parent_id) {
        return;
    }
    let Some(uuid) = extract_uuid_from_title(name) else {
        return;
    };
    data.evidence_threads.write().unwrap().insert(
        uuid,
        EvidenceThread {
            id: thread_id,
            name: display_name_from_title(name).to_string(),
        },
    );
}

pub fn thread_index_remove(data: &Data, thread_id: ThreadId) {
    data.evidence_threads
        .write()
        .unwrap()
        .retain(|_, e| e.id != thread_id);
}

pub async fn populate_thread_index(ctx: &Context, data: &Data) {
    let Some(forum_id) = data.evidence_forum_id else {
        return;
    };
    let Some(guild_id) = data.home_guild_id else {
        return;
    };

    let mut found: HashMap<String, EvidenceThread> = HashMap::new();

    match ctx.http.get_guild_active_threads(guild_id).await {
        Ok(active) => {
            for t in &active.threads {
                if t.parent_id == forum_id {
                    if let Some(uuid) = extract_uuid_from_title(&t.base.name) {
                        found.insert(
                            uuid,
                            EvidenceThread {
                                id: t.id,
                                name: display_name_from_title(&t.base.name).to_string(),
                            },
                        );
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
                        found.insert(
                            uuid,
                            EvidenceThread {
                                id: t.id,
                                name: display_name_from_title(&t.base.name).to_string(),
                            },
                        );
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

fn newest_first(
    mut threads: Vec<(String, EvidenceThread)>,
    limit: usize,
) -> Vec<(String, EvidenceThread)> {
    threads.sort_by_key(|(_, e)| std::cmp::Reverse(e.id.get()));
    threads.truncate(limit);
    threads
}

pub(super) fn recent_evidence_threads(data: &Data, limit: usize) -> Vec<(String, EvidenceThread)> {
    let index = data.evidence_threads.read().unwrap();
    newest_first(
        index.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        limit,
    )
}

fn thread_matches(name: &str, uuid: &str, query: &str) -> bool {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    name.to_ascii_lowercase().contains(&needle) || uuid.contains(&needle.replace('-', ""))
}

pub(super) fn search_evidence_threads(
    data: &Data,
    query: &str,
    limit: usize,
) -> Vec<(String, EvidenceThread)> {
    let index = data.evidence_threads.read().unwrap();
    newest_first(
        index
            .iter()
            .filter(|(uuid, e)| thread_matches(&e.name, uuid, query))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        limit,
    )
}

pub(super) fn evidence_thread_entry(data: &Data, uuid: &str) -> Option<EvidenceThread> {
    let key = uuid.replace('-', "").to_ascii_lowercase();
    data.evidence_threads.read().unwrap().get(&key).cloned()
}

async fn thread_is_archived(ctx: &Context, thread_id: ThreadId) -> bool {
    match ctx.http.get_channel(thread_id.into()).await {
        Ok(channel) => channel
            .thread()
            .is_some_and(|t| t.thread_metadata.archived()),
        Err(e) => {
            tracing::warn!("could not read evidence thread {}: {e}", thread_id.get());
            false
        }
    }
}

pub(super) enum ThreadState {
    Open,
    IdleArchived,
    Closed,
    Missing,
}

pub(super) async fn evidence_thread_state(ctx: &Context, thread_id: ThreadId) -> ThreadState {
    let Ok(channel) = ctx.http.get_channel(thread_id.into()).await else {
        return ThreadState::Missing;
    };
    let Some(thread) = channel.thread() else {
        return ThreadState::Missing;
    };
    if thread.thread_metadata.locked() {
        ThreadState::Closed
    } else if thread.thread_metadata.archived() {
        ThreadState::IdleArchived
    } else {
        ThreadState::Open
    }
}

pub fn evidence_thread_url(data: &Data, uuid: &str) -> Option<String> {
    let thread_id = data.evidence_thread_for(uuid)?;
    let guild_id = data.home_guild_id?;
    Some(format!(
        "https://discord.com/channels/{guild_id}/{thread_id}"
    ))
}
pub(super) const ALLOWED_MEDIA_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm", "mov"];
const MAX_EVIDENCE_MEDIA: u8 = 10;
pub(super) const MAX_EVIDENCE_ITEMS: usize = 9;

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
    let (rank, member) = get_rank_and_member(data, discord_id).await?;
    let can_confirm = rank >= AccessRank::Helper
        || member
            .as_ref()
            .is_some_and(|m| database::standing::evaluate(m).can_tag);
    if !can_confirm {
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

    if let Some(thread_id) = data.evidence_thread_for(&player_info.uuid) {
        let thread_url = evidence_thread_url(data, &player_info.uuid)
            .unwrap_or_else(|| format!("<#{}>", thread_id.get()));
        let emote = lookup_tag("confirmed_cheater")
            .map(|d| d.emote)
            .unwrap_or("");
        let archived = thread_is_archived(ctx, thread_id).await;

        let mut parts = vec![face_section(format!(
            "## {} Evidence {}\nIGN - `{}`\nThread: {}",
            emote,
            if archived {
                "Post Archived"
            } else {
                "Already Exists"
            },
            player_info.username,
            thread_url
        ))];
        if archived {
            parts.push(text(if rank >= AccessRank::Helper {
                "-# Restoring reopens the post and re-applies the confirmed cheater tag."
            } else {
                "-# A staff member has to restore this post before it can be confirmed again."
            }));
            if rank >= AccessRank::Helper {
                parts.push(CreateContainerComponent::ActionRow(
                    CreateActionRow::buttons(vec![
                        CreateButton::new(format!("evidence_restore:{}", player_info.uuid))
                            .label("Restore Post")
                            .style(ButtonStyle::Primary),
                    ]),
                ));
            }
        }

        command
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(vec![CreateComponent::Container(CreateContainer::new(
                        parts,
                    ))])
                    .new_attachment(face_attachment(data, &player_info.uuid).await),
            )
            .await?;
        return Ok(());
    }

    if rank < AccessRank::Helper {
        return run_member_confirm(ctx, command, data, discord_id, &player_info, tag).await;
    }

    let reason = tag.reason.clone().unwrap_or_default();
    let added_line = format_added_line(ctx, tag).await;
    let reviewed_line = format_reviewed_line(ctx, tag.reviewed_by.as_deref()).await;
    run_staff_confirm(
        ctx,
        command,
        data,
        &player_info,
        &reason,
        Some(&added_line),
        reviewed_line.as_deref(),
    )
    .await
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
    let author_name = match tag.author.filter(|_| !tag.hide_username.unwrap_or(false)) {
        Some(author) if author as u64 != discord_id => {
            Some(super::channel::get_username(ctx, author as u64).await)
        }
        _ => None,
    };
    let thread_id = reviews::create_submission(
        ctx,
        data,
        discord_id,
        &player_info.username,
        &player_info.uuid,
        tag.tag_type.as_deref().unwrap_or(""),
        reason,
        author_name.as_deref(),
    )
    .await?;

    reviews::spawn_submission_timeout(ctx.clone(), data.clone(), thread_id);

    let emote = lookup_tag("confirmed_cheater")
        .map(|d| d.emote)
        .unwrap_or("");
    command.edit_response(
        &ctx.http,
        EditInteractionResponse::new()
            .flags(MessageFlags::IS_COMPONENTS_V2)
            .components(vec![CreateComponent::Container(CreateContainer::new(
                vec![face_section(format!(
                    "## {} Review Submitted\nIGN - `{}`\nThread: <#{}>\n-# Add evidence to the thread to proceed",
                    emote, player_info.username, thread_id.get()
                ))],
            ))])
            .new_attachment(face_attachment(data, &player_info.uuid).await),
    ).await?;
    Ok(())
}

async fn run_staff_confirm(
    ctx: &Context,
    command: &CommandInteraction,
    data: &Data,
    player_info: &crate::api::ResolveResponse,
    reason: &str,
    added_line: Option<&str>,
    reviewed_line: Option<&str>,
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
        added_line,
        reviewed_line,
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
                    vec![face_section(format!(
                        "## {} Evidence Post Created\nIGN - `{}`\nThread: <#{}>",
                        emote,
                        player_info.username,
                        thread.id.get()
                    ))],
                ))])
                .new_attachment(face_attachment(data, &player_info.uuid).await),
        )
        .await?;
    Ok(())
}

async fn evidence_added_line(ctx: &Context, data: &Data, uuid: &str) -> Option<String> {
    let tags = BlacklistRepository::new(data.db.pool())
        .get_active_tags(uuid)
        .await
        .ok()?;
    let tag = tags
        .iter()
        .find(|t| t.tag_type.as_deref() == Some("confirmed_cheater"))?;
    Some(format_added_line(ctx, tag).await)
}

#[derive(Debug, Clone)]
struct EvidenceItem {
    filename: String,
}

struct EvidenceState {
    username: String,
    uuid: String,
    reason: String,
    added_line: Option<String>,
    reviewed_line: Option<String>,
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

pub(super) async fn face_attachment(data: &Data, uuid: &str) -> CreateAttachment<'static> {
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

#[allow(clippy::too_many_arguments)]
fn build_evidence_message(
    username: &str,
    uuid: &str,
    reason: &str,
    added_line: Option<&str>,
    reviewed_line: Option<&str>,
    evidence: &[EvidenceItem],
    review_thread_url: Option<&str>,
    gallery_urls: &HashMap<String, String>,
) -> Vec<CreateComponent<'static>> {
    let dashed_uuid = format_uuid_dashed(uuid);

    let block = format_tag_block(
        "confirmed_cheater",
        &sanitize_reason(reason),
        "",
        added_line,
        reviewed_line,
        false,
    );

    let mut uuid_footer = format!("-# UUID: {dashed_uuid}");
    if let Some(url) = review_thread_url {
        uuid_footer.push_str(&format!(" · [Review]({url})"));
    }

    let header_emote = if evidence.is_empty() {
        EMOTE_NO_EVIDENCE
    } else {
        EMOTE_EVIDENCE
    };
    let mut parts: Vec<CreateContainerComponent<'static>> = Vec::new();
    parts.push(text(format!("## {header_emote} Evidence — `{username}`")));
    if evidence.is_empty() {
        parts.push(text("-# No evidence added yet"));
    } else {
        parts.push(media_gallery(evidence, gallery_urls));
    }
    parts.push(face_section(format!("{block}\n{uuid_footer}")));
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(format!("tag_history:{uuid}"))
                    .label("Tag History")
                    .style(ButtonStyle::Secondary),
            ]
            .into(),
        ),
    ));
    parts.push(separator());

    let mut buttons = vec![
        CreateButton::new("evidence_add_media")
            .label("Add Media")
            .style(ButtonStyle::Primary),
    ];
    if !evidence.is_empty() {
        buttons.push(
            CreateButton::new(format!("evidence_manage:{uuid}"))
                .label("Remove Media")
                .style(ButtonStyle::Secondary),
        );
    }
    buttons.push(
        CreateButton::new(format!("evidence_change:{uuid}"))
            .label("Change Tag")
            .style(ButtonStyle::Secondary),
    );
    buttons.push(
        CreateButton::new("evidence_archive")
            .label("Archive")
            .style(ButtonStyle::Danger),
    );
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(buttons.into()),
    ));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn evidence_name_index(filename: &str) -> (&str, Option<&str>) {
    let stem = filename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(filename);
    match stem.rsplit_once('_') {
        Some((name, n)) => (name, Some(n)),
        None => (stem, None),
    }
}

fn evidence_label(filename: &str) -> String {
    match evidence_name_index(filename) {
        (name, Some(n)) => format!("{name} ({n})"),
        (name, None) => name.to_string(),
    }
}

fn evidence_label_code(filename: &str) -> String {
    match evidence_name_index(filename) {
        (name, Some(n)) => format!("`{name}` ({n})"),
        (name, None) => format!("`{name}`"),
    }
}

fn build_evidence_manage_container(
    uuid: &str,
    opener: u64,
    evidence: &[EvidenceItem],
    selected: &str,
    gallery_urls: &HashMap<String, String>,
) -> CreateComponent<'static> {
    let url = gallery_urls
        .get(selected)
        .cloned()
        .unwrap_or_else(|| format!("attachment://{selected}"));
    let mut parts = vec![
        text(format!(
            "## Remove Media\n{}",
            evidence_label_code(selected)
        )),
        CreateContainerComponent::MediaGallery(CreateMediaGallery::new(vec![
            CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url)),
        ])),
    ];
    if evidence.len() > 1 {
        let options: Vec<CreateSelectMenuOption<'static>> = evidence
            .iter()
            .filter(|e| e.filename != selected)
            .map(|e| CreateSelectMenuOption::new(evidence_label(&e.filename), e.filename.clone()))
            .collect();
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::SelectMenu(
                CreateSelectMenu::new(
                    format!("evidence_msel:{uuid}:{opener}"),
                    CreateSelectMenuKind::String {
                        options: options.into(),
                    },
                )
                .placeholder("View another piece..."),
            ),
        ));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(format!("evidence_mrem:{uuid}:{opener}:{selected}"))
                    .label("Remove")
                    .style(ButtonStyle::Danger),
                CreateButton::new(format!("evidence_mclose:{uuid}:{opener}"))
                    .label("Done")
                    .style(ButtonStyle::Secondary),
            ]
            .into(),
        ),
    ));
    CreateComponent::Container(CreateContainer::new(parts))
}

fn media_gallery(
    evidence: &[EvidenceItem],
    gallery_urls: &HashMap<String, String>,
) -> CreateContainerComponent<'static> {
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
    CreateContainerComponent::MediaGallery(CreateMediaGallery::new(items))
}

fn build_archived_evidence_message(
    state: &EvidenceState,
    gallery_urls: &HashMap<String, String>,
) -> Vec<CreateComponent<'static>> {
    let dashed_uuid = format_uuid_dashed(&state.uuid);
    let mut footer = format!("-# UUID: {dashed_uuid}");
    if let Some(url) = &state.review_url {
        footer.push_str(&format!(" · [Review]({url})"));
    }

    let header_emote = if state.evidence.is_empty() {
        EMOTE_NO_EVIDENCE
    } else {
        EMOTE_EVIDENCE
    };
    let mut parts: Vec<CreateContainerComponent<'static>> = vec![face_section(format!(
        "## {header_emote} Evidence — `{}` (Archived)\n{footer}",
        state.username
    ))];
    if !state.evidence.is_empty() {
        parts.push(media_gallery(&state.evidence, gallery_urls));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(format!("tag_history:{}", state.uuid))
                    .label("Tag History")
                    .style(ButtonStyle::Secondary),
            ]
            .into(),
        ),
    ));
    parts.push(separator());

    vec![CreateComponent::Container(CreateContainer::new(parts))]
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
        added_line: None,
        reviewed_line: None,
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
        } else if let Some(rest) = trimmed.strip_prefix("> -# ") {
            if rest.contains("Reviewed by ") {
                state.reviewed_line = Some(trimmed.to_string());
            } else if rest.starts_with("**\\-") && state.added_line.is_none() {
                state.added_line = Some(trimmed.to_string());
            }
        } else if state.reason.is_empty() {
            if let Some(rest) = trimmed.strip_prefix("> ") {
                if !rest.starts_with("-#") {
                    state.reason = unsanitize_reason(rest);
                }
            }
        }
    }
}

fn extract_evidence_name(header: &str) -> Option<String> {
    let after = header.split(" — `").nth(1)?;
    Some(
        after
            .trim_end_matches(" (Archived)")
            .trim_end_matches('`')
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
    let Some(tag) = tags.iter().find(|t| {
        matches!(
            t.tag_type.as_deref(),
            Some("closet_cheater" | "blatant_cheater")
        )
    }) else {
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
            Some(actor_id as i64),
            None,
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
                silent: false,
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

pub(super) enum AppendError {
    NotFound,
    Legacy,
    Full,
    NoneAccepted,
    DownloadFailed,
    TooLarge,
    UploadFailed,
}

impl AppendError {
    pub(super) fn message(&self) -> String {
        match self {
            AppendError::NotFound => "Could not find the evidence message".into(),
            AppendError::Legacy => {
                "That post predates the current bot, so it can't be edited. Add media from inside the thread.".into()
            }
            AppendError::Full => format!(
                "That evidence post is already at the {MAX_EVIDENCE_ITEMS} attachment limit. Remove something first."
            ),
            AppendError::NoneAccepted => {
                "Only images and videos are accepted (png, jpg, gif, webp, mp4, webm, mov)".into()
            }
            AppendError::DownloadFailed => "Could not download those attachments. Please try again.".into(),
            AppendError::TooLarge => "File too large. Try compressing or using a smaller file.".into(),
            AppendError::UploadFailed => "Failed to upload evidence. Please try again.".into(),
        }
    }
}

pub(super) struct AppendOutcome {
    pub added: usize,
    pub rejected: usize,
    pub failed: usize,
    pub over_capacity: usize,
    pub username: String,
}

impl AppendOutcome {
    pub(super) fn notes(&self) -> String {
        let mut out = String::new();
        if self.over_capacity > 0 {
            out.push_str(&format!(
                "\n-# {} skipped — the post holds at most {MAX_EVIDENCE_ITEMS} attachments",
                self.over_capacity
            ));
        }
        if self.rejected > 0 {
            out.push_str(&format!(
                "\n-# {} skipped — not an accepted image or video",
                self.rejected
            ));
        }
        if self.failed > 0 {
            out.push_str(&format!("\n-# {} failed to download", self.failed));
        }
        out
    }
}

fn next_evidence_index(evidence: &[EvidenceItem]) -> usize {
    evidence
        .iter()
        .filter_map(|e| match evidence_name_index(&e.filename) {
            (_, Some(n)) => n.parse::<usize>().ok(),
            _ => None,
        })
        .max()
        .unwrap_or(evidence.len())
        + 1
}

pub(super) async fn append_media_to_op(
    ctx: &Context,
    data: &Data,
    thread_id: ThreadId,
    actor_id: u64,
    sources: &[(String, String)],
) -> std::result::Result<AppendOutcome, AppendError> {
    let channel_id: GenericChannelId = thread_id.into();
    let op_id = MessageId::new(thread_id.get());

    let Ok(op) = ctx.http.get_message(channel_id, op_id).await else {
        return Err(AppendError::NotFound);
    };
    if op.author.id != ctx.cache.current_user().id {
        return Err(AppendError::Legacy);
    }
    let Some(mut state) = parse_state_from_message(&op) else {
        return Err(AppendError::Legacy);
    };

    let existing_count = state.evidence.len();
    let capacity = MAX_EVIDENCE_ITEMS.saturating_sub(existing_count);
    if capacity == 0 {
        return Err(AppendError::Full);
    }

    let mut next_index = next_evidence_index(&state.evidence);
    let mut files = Vec::new();
    let mut rejected = 0usize;
    let mut failed = 0usize;
    let mut over_capacity = 0usize;

    for (source_name, url) in sources {
        let ext = url_extension(source_name).to_ascii_lowercase();
        if !ALLOWED_MEDIA_EXTENSIONS.contains(&ext.as_str()) {
            rejected += 1;
            continue;
        }
        if files.len() >= capacity {
            over_capacity += 1;
            continue;
        }
        let filename = format!("{}_{}.{}", state.username, next_index, ext);
        match CreateAttachment::url(&ctx.http, url.as_str(), filename.clone()).await {
            Ok(file) => {
                files.push(file);
                state.evidence.push(EvidenceItem { filename });
                next_index += 1;
            }
            Err(e) => {
                tracing::warn!("Failed to download attachment: {e}");
                failed += 1;
            }
        }
    }

    if files.is_empty() {
        return Err(if over_capacity > 0 {
            AppendError::Full
        } else if failed > 0 {
            AppendError::DownloadFailed
        } else {
            AppendError::NoneAccepted
        });
    }

    let urls = gallery_url_map(&op);
    let components = evidence_op_view(&state, &urls);

    let face = face_attachment(data, &state.uuid).await;
    let mut attachments = EditAttachments::new();
    for url in urls.values() {
        if let Some(id) = attachment_id_from_cdn_url(url) {
            attachments = attachments.keep(id);
        }
    }
    for f in files.iter().cloned() {
        attachments = attachments.add(f);
    }
    attachments = attachments.add(face.clone());

    let mut all_files = files.clone();
    all_files.push(face);

    let edit = EditMessage::new()
        .content("")
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components)
        .attachments(attachments);

    if let Err(e) = ctx
        .http
        .edit_message(channel_id, op_id, &edit, all_files)
        .await
    {
        let msg = e.to_string();
        return Err(if msg.contains("too large") || msg.contains("413") {
            AppendError::TooLarge
        } else {
            AppendError::UploadFailed
        });
    }

    if existing_count == 0 {
        if let Err(e) = try_convert_to_confirmed(data, &state, actor_id).await {
            tracing::warn!("failed to convert tag to confirmed: {e}");
        }
    }

    Ok(AppendOutcome {
        added: files.len(),
        rejected,
        failed,
        over_capacity,
        username: state.username,
    })
}

pub async fn handle_media_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    modal.defer_ephemeral(&ctx.http).await?;

    let sources: Vec<(String, String)> = modal
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
        .filter_map(|id| modal.data.resolved.attachments.get(&id))
        .map(|a| (a.filename.to_string(), a.url.to_string()))
        .collect();

    if sources.is_empty() {
        let _ = modal.delete_response(&ctx.http).await;
        return Ok(());
    }

    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().content("Uploading evidence..."),
        )
        .await?;

    let thread_id = ThreadId::new(modal.channel_id.get());
    match append_media_to_op(ctx, data, thread_id, modal.user.id.get(), &sources).await {
        Ok(outcome) => {
            let notes = outcome.notes();
            if notes.is_empty() {
                let _ = modal.delete_response(&ctx.http).await;
            } else {
                modal
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new().content(format!(
                            "Added {} attachment{}.{notes}",
                            outcome.added,
                            if outcome.added == 1 { "" } else { "s" }
                        )),
                    )
                    .await?;
            }
        }
        Err(e) => {
            modal
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new().content(e.message()),
                )
                .await?;
        }
    }
    Ok(())
}

async fn rebuild_evidence_op(
    ctx: &Context,
    data: &Data,
    channel_id: GenericChannelId,
    message_id: MessageId,
    uuid: &str,
    components: Vec<CreateComponent<'static>>,
    urls: &HashMap<String, String>,
) -> Result<()> {
    let face = face_attachment(data, uuid).await;
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
    ctx.http
        .edit_message(channel_id, message_id, &edit, vec![face])
        .await?;
    Ok(())
}

async fn manage_locked_error(ctx: &Context, component: &ComponentInteraction) -> Result<()> {
    crate::interact::send_component_error(
        ctx,
        component,
        "Error",
        "Only the person managing this evidence can use these controls",
    )
    .await
}

fn evidence_op_view(
    state: &EvidenceState,
    urls: &HashMap<String, String>,
) -> Vec<CreateComponent<'static>> {
    build_evidence_message(
        &state.username,
        &state.uuid,
        &state.reason,
        state.added_line.as_deref(),
        state.reviewed_line.as_deref(),
        &state.evidence,
        state.review_url.as_deref(),
        urls,
    )
}

pub async fn handle_manage(
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
            "Only staff can remove evidence",
        )
        .await;
    }
    let Some(state) = parse_state_from_message(&component.message) else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Could not parse evidence state",
        )
        .await;
    };
    let Some(first) = state.evidence.first() else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "No evidence to remove",
        )
        .await;
    };

    let urls = gallery_url_map(&component.message);
    let mut components = evidence_op_view(&state, &urls);
    components.push(build_evidence_manage_container(
        &state.uuid,
        discord_id,
        &state.evidence,
        &first.filename,
        &urls,
    ));
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    rebuild_evidence_op(
        ctx,
        data,
        component.channel_id.into(),
        component.message.id,
        &state.uuid,
        components,
        &urls,
    )
    .await
}

pub async fn handle_manage_select(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let rest = component
        .data
        .custom_id
        .strip_prefix("evidence_msel:")
        .unwrap_or_default();
    let opener: u64 = rest.rsplit(':').next().unwrap_or("").parse().unwrap_or(0);
    if component.user.id.get() != opener {
        return manage_locked_error(ctx, component).await;
    }
    let selected = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().cloned().unwrap_or_default()
        }
        _ => return Ok(()),
    };
    let Some(state) = parse_state_from_message(&component.message) else {
        return Ok(());
    };
    let urls = gallery_url_map(&component.message);
    let mut components = evidence_op_view(&state, &urls);
    components.push(build_evidence_manage_container(
        &state.uuid,
        opener,
        &state.evidence,
        &selected,
        &urls,
    ));
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    rebuild_evidence_op(
        ctx,
        data,
        component.channel_id.into(),
        component.message.id,
        &state.uuid,
        components,
        &urls,
    )
    .await
}

pub async fn handle_manage_remove(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let discord_id = component.user.id.get();
    let rest = component
        .data
        .custom_id
        .strip_prefix("evidence_mrem:")
        .unwrap_or_default();
    let mut segs = rest.splitn(3, ':');
    let _uuid = segs.next().unwrap_or("");
    let opener: u64 = segs.next().unwrap_or("").parse().unwrap_or(0);
    let filename = segs.next().unwrap_or("");
    if discord_id != opener {
        return manage_locked_error(ctx, component).await;
    }
    let rank = get_rank(data, discord_id).await?;
    if rank < AccessRank::Helper {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only staff can remove evidence",
        )
        .await;
    }
    let Some(mut state) = parse_state_from_message(&component.message) else {
        return Ok(());
    };
    state.evidence.retain(|e| e.filename != filename);
    let mut urls = gallery_url_map(&component.message);
    urls.remove(filename);

    let components = evidence_op_view(&state, &urls);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    rebuild_evidence_op(
        ctx,
        data,
        component.channel_id.into(),
        component.message.id,
        &state.uuid,
        components,
        &urls,
    )
    .await
}

pub async fn handle_manage_close(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let opener: u64 = component
        .data
        .custom_id
        .rsplit(':')
        .next()
        .unwrap_or("")
        .parse()
        .unwrap_or(0);
    if component.user.id.get() != opener {
        return manage_locked_error(ctx, component).await;
    }
    let Some(state) = parse_state_from_message(&component.message) else {
        return Ok(());
    };
    let urls = gallery_url_map(&component.message);
    let components = evidence_op_view(&state, &urls);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    rebuild_evidence_op(
        ctx,
        data,
        component.channel_id.into(),
        component.message.id,
        &state.uuid,
        components,
        &urls,
    )
    .await
}

async fn remove_confirmed_tag(
    repo: &BlacklistRepository<'_>,
    data: &Data,
    state: &EvidenceState,
    actor: i64,
) -> Result<()> {
    let tag = repo
        .get_active_tag(&state.uuid, "confirmed_cheater")
        .await?;
    if !repo
        .remove_event(&state.uuid, "confirmed_cheater", Some(actor))
        .await?
    {
        return Ok(());
    }

    if let Some(tag) = tag {
        data.event_publisher
            .publish(&BlacklistEvent::TagRemoved {
                uuid: state.uuid.clone(),
                tag_id: tag.id,
                removed_by: actor,
                silent: true,
            })
            .await;
    }
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
            "Only staff can archive evidence",
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
    remove_confirmed_tag(&repo, data, &state, discord_id as i64).await?;

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

async fn deferred_component_error(
    ctx: &Context,
    component: &ComponentInteraction,
    message: &str,
) -> Result<()> {
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(CreateContainer::new(
                    vec![text(format!("## Error\n{message}"))],
                ))]),
        )
        .await?;
    Ok(())
}

/// Reopens an archived evidence post and re-applies the confirmed cheater tag.
pub async fn handle_restore(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let uuid = component
        .data
        .custom_id
        .strip_prefix("evidence_restore:")
        .unwrap_or("")
        .to_string();

    let discord_id = component.user.id.get();
    if get_rank(data, discord_id).await? < AccessRank::Helper {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only staff can restore evidence",
        )
        .await;
    }

    let Some(thread_id) = data.evidence_thread_for(&uuid) else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "That evidence post no longer exists",
        )
        .await;
    };

    let repo = BlacklistRepository::new(data.db.pool());
    let tags = repo.get_active_tags(&uuid).await?;
    if !tags.iter().any(|t| {
        matches!(
            t.tag_type.as_deref(),
            Some("confirmed_cheater" | "closet_cheater" | "blatant_cheater")
        )
    }) {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Player must have a closet cheater or blatant cheater tag",
        )
        .await;
    }

    component.defer(&ctx.http).await?;

    let channel_id: GenericChannelId = thread_id.into();
    let op_id = MessageId::new(thread_id.get());
    thread_id
        .edit(&ctx.http, EditThread::new().archived(false).locked(false))
        .await?;

    let op = ctx.http.get_message(channel_id, op_id).await?;
    let Some(state) = parse_state_from_message(&op) else {
        return deferred_component_error(ctx, component, "Could not parse evidence state").await;
    };

    try_convert_to_confirmed(data, &state, discord_id).await?;

    let confirmed = repo.get_active_tag(&uuid, "confirmed_cheater").await?;
    let reason = confirmed
        .as_ref()
        .and_then(|t| t.reason.clone())
        .unwrap_or_else(|| state.reason.clone());
    let added_line = match &confirmed {
        Some(tag) => Some(format_added_line(ctx, tag).await),
        None => state.added_line.clone(),
    };
    let reviewed_line = match confirmed.as_ref().map(|t| t.reviewed_by.as_deref()) {
        Some(reviewed) => format_reviewed_line(ctx, reviewed).await,
        None => state.reviewed_line.clone(),
    };

    let urls = gallery_url_map(&op);
    let components = build_evidence_message(
        &state.username,
        &uuid,
        &reason,
        added_line.as_deref(),
        reviewed_line.as_deref(),
        &state.evidence,
        state.review_url.as_deref(),
        &urls,
    );
    rebuild_evidence_op(ctx, data, channel_id, op_id, &uuid, components, &urls).await?;

    let emote = lookup_tag("confirmed_cheater")
        .map(|d| d.emote)
        .unwrap_or("");
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(CreateContainer::new(
                    vec![face_section(format!(
                        "## {} Evidence Post Restored\nIGN - `{}`\nThread: <#{}>",
                        emote,
                        state.username,
                        thread_id.get()
                    ))],
                ))])
                .new_attachment(face_attachment(data, &uuid).await),
        )
        .await?;
    Ok(())
}

/// Opens the reason prompt for replacing the confirmed cheater tag behind an
/// evidence post.
pub async fn handle_change_tag(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let uuid = component
        .data
        .custom_id
        .strip_prefix("evidence_change:")
        .unwrap_or("");

    let discord_id = component.user.id.get();
    if get_rank(data, discord_id).await? < AccessRank::Helper {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only staff can change a confirmed cheater tag",
        )
        .await;
    }

    let repo = BlacklistRepository::new(data.db.pool());
    let Some(tag) = repo.get_active_tag(uuid, "confirmed_cheater").await? else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "This player has no confirmed cheater tag to change",
        )
        .await;
    };

    let input = CreateInputText::new(InputTextStyle::Paragraph, "evidence_reason")
        .placeholder("Reason for this tag")
        .value(tag.reason.unwrap_or_default())
        .max_length(120)
        .required(true);
    let modal =
        CreateModal::new(format!("evidence_change_modal:{uuid}"), "Change Tag").components(vec![
            CreateModalComponent::Label(CreateLabel::input_text("Reason", input)),
        ]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_change_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    modal.defer_ephemeral(&ctx.http).await?;

    let uuid = modal
        .data
        .custom_id
        .strip_prefix("evidence_change_modal:")
        .unwrap_or("")
        .to_string();
    let reason = crate::interact::extract_modal_value(&modal.data.components, "evidence_reason");
    let discord_id = modal.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    let repo = BlacklistRepository::new(data.db.pool());
    let hide = repo
        .get_active_tag(&uuid, "confirmed_cheater")
        .await?
        .and_then(|t| t.hide_username)
        .unwrap_or(false);

    let ops = database::TagOp::new(data.db.pool());
    let (old_tag, new_tag) = match ops
        .overwrite(
            &uuid,
            "confirmed_cheater",
            "confirmed_cheater",
            &reason,
            discord_id as i64,
            rank.to_level(),
            hide,
        )
        .await
    {
        Ok(tags) => tags,
        Err(e) => {
            modal
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new().content(super::tag::op_error_message(&e)),
                )
                .await?;
            return Ok(());
        }
    };

    data.event_publisher
        .publish(&BlacklistEvent::TagOverwritten {
            uuid: uuid.clone(),
            old_tag_id: old_tag.id,
            old_tag_type: old_tag.tag_type.clone().unwrap_or_default(),
            old_reason: old_tag.reason.clone().unwrap_or_default(),
            new_tag_id: new_tag.id,
            overwritten_by: discord_id as i64,
            silent: false,
        })
        .await;

    let channel_id = modal.channel_id;
    let op_id = MessageId::new(channel_id.get());
    if let Ok(op) = ctx.http.get_message(channel_id, op_id).await
        && let Some(state) = parse_state_from_message(&op)
    {
        let urls = gallery_url_map(&op);
        let added_line = format_added_line(ctx, &new_tag).await;
        let components = build_evidence_message(
            &state.username,
            &uuid,
            &reason,
            Some(&added_line),
            state.reviewed_line.as_deref(),
            &state.evidence,
            state.review_url.as_deref(),
            &urls,
        );
        rebuild_evidence_op(ctx, data, channel_id, op_id, &uuid, components, &urls).await?;
    }

    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().content("Confirmed cheater tag replaced"),
        )
        .await?;
    Ok(())
}

pub async fn refresh_evidence_reason(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    tag: &database::PlayerEvent,
) -> Result<()> {
    let Some(thread_id) = data.evidence_thread_for(uuid) else {
        return Ok(());
    };
    let channel_id: GenericChannelId = thread_id.into();
    let op_id = MessageId::new(thread_id.get());

    let Ok(op) = ctx.http.get_message(channel_id, op_id).await else {
        return Ok(());
    };
    let Some(state) = parse_state_from_message(&op) else {
        return Ok(());
    };

    let urls = gallery_url_map(&op);
    let added_line = format_added_line(ctx, tag).await;
    let components = build_evidence_message(
        &state.username,
        uuid,
        tag.reason.as_deref().unwrap_or(""),
        Some(&added_line),
        state.reviewed_line.as_deref(),
        &state.evidence,
        state.review_url.as_deref(),
        &urls,
    );
    rebuild_evidence_op(ctx, data, channel_id, op_id, uuid, components, &urls).await
}

pub async fn create_evidence_from_review(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    username: &str,
    reason: &str,
    media_urls: &[String],
    review_thread_url: Option<&str>,
    reviewer_names: &[String],
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
    let added_line = evidence_added_line(ctx, data, uuid).await;
    let reviewed_line = (!reviewer_names.is_empty()).then(|| {
        let names = reviewer_names
            .iter()
            .map(|n| format!("`@{n}`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("> -# **\\- Reviewed by {names}**")
    });
    let initial_components = build_evidence_message(
        username,
        uuid,
        reason,
        added_line.as_deref(),
        reviewed_line.as_deref(),
        &[],
        review_thread_url,
        &no_urls,
    );

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
                added_line.as_deref(),
                reviewed_line.as_deref(),
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
