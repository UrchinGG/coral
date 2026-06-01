use std::collections::HashMap;

use blacklist::{EMOTE_EVIDENCE, EMOTE_NO_EVIDENCE, Replay, parse_replay};
use serenity::all::*;

use super::*;

#[derive(Debug, Clone)]
pub struct PlayerEntry {
    pub username: String,
    pub uuid: String,
    pub tag_type: String,
    pub reason: String,
    pub status: PlayerStatus,
    pub reviewer: Option<String>,
    pub review_note: Option<String>,
    pub evidence: Vec<Evidence>,
    pub conflict_warning: Option<String>,
    pub accept_votes: Vec<u64>,
    pub reject_votes: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone)]
pub enum Evidence {
    Replay {
        replay: Replay,
        note: Option<String>,
    },
    Attachment {
        filename: String,
    },
}

#[derive(Debug, Clone)]
pub struct PendingAdd {
    pub identifier: String,
    pub username: String,
}

#[derive(Debug, Clone)]
pub struct SubmissionState {
    pub submitter_id: u64,
    pub players: Vec<PlayerEntry>,
    pub submitted: bool,
    pub reopened: bool,
    pub editing: Option<usize>,
    pub pending_add: Option<PendingAdd>,
}

pub struct ForumTags {
    pub pending: Option<ForumTagId>,
    pub approved: Option<ForumTagId>,
    pub rejected: Option<ForumTagId>,
    pub awaiting_evidence: Option<ForumTagId>,
}

pub struct ConfirmationData {
    pub player_name: String,
    pub player_uuid: String,
    pub tag_type: String,
    pub reason: String,
}

pub fn parse_state_from_message(message: &Message) -> Option<SubmissionState> {
    let container = find_container(message)?;
    let texts = extract_text_displays(message);

    let submitter_id = texts.iter().find_map(|t| {
        let start = t.find("<@")? + 2;
        let end = t[start..].find('>')? + start;
        t[start..end].parse::<u64>().ok()
    })?;

    let blocks = split_into_blocks(container);
    let mut players: Vec<PlayerEntry> = Vec::new();
    for block in &blocks {
        if let Some(player) = parse_player_block(block) {
            players.push(player);
        }
    }

    let submitted = texts
        .iter()
        .any(|t| t.contains("Approved") || t.contains("Rejected") || t.contains("Vote"))
        || container.components.iter().any(|c| match c {
            ContainerComponent::ActionRow(row) => row.components.iter().any(|b| match b {
                ActionRowComponent::Button(btn) => match &btn.data {
                    ButtonKind::NonLink { custom_id, .. } => {
                        custom_id.starts_with("review_approve:")
                            || custom_id.starts_with("review_reject:")
                    }
                    _ => false,
                },
                _ => false,
            }),
            _ => false,
        });

    let reopened = texts
        .iter()
        .any(|t| t.starts_with("-# Submitted by") && t.contains("reopened"));

    let players: Vec<_> = players
        .into_iter()
        .filter(|p| !p.tag_type.is_empty())
        .collect();

    Some(SubmissionState {
        submitter_id,
        players,
        submitted,
        reopened,
        editing: None,
        pending_add: None,
    })
}

fn split_into_blocks<'a>(
    container: &'a serenity::all::Container,
) -> Vec<Vec<&'a ContainerComponent>> {
    let mut blocks = vec![Vec::new()];
    for part in &*container.components {
        if matches!(part, ContainerComponent::Separator(_)) {
            blocks.push(Vec::new());
        } else {
            blocks.last_mut().unwrap().push(part);
        }
    }
    blocks
}

fn parse_player_block(block: &[&ContainerComponent]) -> Option<PlayerEntry> {
    let username = block.iter().find_map(|c| match c {
        ContainerComponent::TextDisplay(td) => td
            .content
            .as_deref()
            .and_then(|c| c.lines().find_map(|line| parse_player_ign(line.trim()))),
        ContainerComponent::Section(section) => section.components.iter().find_map(|sc| match sc {
            SectionComponent::TextDisplay(td) => td
                .content
                .as_deref()
                .and_then(|c| c.lines().find_map(|line| parse_player_ign(line.trim()))),
            _ => None,
        }),
        _ => None,
    })?;

    let mut player = new_player_entry(username, "");

    for part in block {
        match part {
            ContainerComponent::TextDisplay(td) => {
                if let Some(content) = &td.content {
                    process_text_into_player(&mut player, content);
                }
            }
            ContainerComponent::Section(section) => {
                for sc in &*section.components {
                    if let SectionComponent::TextDisplay(td) = sc {
                        if let Some(content) = &td.content {
                            process_text_into_player(&mut player, content);
                        }
                    }
                }
                if let SectionAccessory::Thumbnail(thumb) = &*section.accessory {
                    let url = thumb.media.url.to_string();
                    if url.contains("/attachments/") {
                        player.evidence.push(Evidence::Attachment {
                            filename: attachment_filename_from_url(&url),
                        });
                    }
                }
            }
            ContainerComponent::MediaGallery(gallery) => {
                for item in &*gallery.items {
                    let filename = attachment_filename_from_url(&item.media.url.to_string());
                    player.evidence.push(Evidence::Attachment { filename });
                }
            }
            _ => {}
        }
    }

    Some(player)
}

fn process_text_into_player(player: &mut PlayerEntry, content: &str) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_player_entry(trimmed) {
            continue;
        }
        if is_tag_type_line(trimmed) {
            if let Some(tag_name) = parse_tag_type_line(trimmed) {
                if player.tag_type.is_empty() {
                    player.tag_type = tag_name.to_string();
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("-# currently ") {
            let display = rest.split('>').next_back().unwrap_or(rest).trim();
            if let Some(name) = lookup_tag_name_from_display(display) {
                player.tag_type = name.to_string();
            }
        } else if let Some(uuid_str) = trimmed.strip_prefix("-# UUID: ") {
            player.uuid = uuid_str
                .split_whitespace()
                .next()
                .unwrap_or("")
                .replace('-', "");
        } else if let Some(status) = parse_status_line(trimmed) {
            player.status = status.0;
            player.reviewer = status.1;
            player.review_note = status.2;
        } else if let Some((side, voters)) = parse_vote_line(trimmed) {
            match side {
                VoteSide::Accept => player.accept_votes = voters,
                VoteSide::Reject => player.reject_votes = voters,
            }
        } else if trimmed.starts_with('>') {
            if player.reason.is_empty() {
                if let Some(reason) = trimmed.strip_prefix("> ") {
                    player.reason = reason.to_string();
                } else if let Some(reason) = trimmed.strip_prefix('>') {
                    player.reason = reason.trim().to_string();
                }
            }
        } else if let Some(evidence) = parse_evidence_line(trimmed) {
            player.evidence.push(evidence);
        }
    }
}

pub fn is_player_entry(text: &str) -> bool {
    text.starts_with("IGN - `")
}

fn is_tag_type_line(text: &str) -> bool {
    text.starts_with("**") && text.ends_with("**") && text.contains('>')
}

fn parse_player_ign(text: &str) -> Option<String> {
    text.strip_prefix("IGN - `")?
        .strip_suffix('`')
        .map(|s| s.to_string())
}

fn parse_tag_type_line(text: &str) -> Option<&'static str> {
    let inner = text.strip_prefix("**")?.strip_suffix("**")?;
    let display = inner.split('>').next_back()?.trim();
    lookup_tag_name_from_display(display)
}

pub enum VoteSide {
    Accept,
    Reject,
}

pub fn parse_vote_line(text: &str) -> Option<(VoteSide, Vec<u64>)> {
    let side = if text.starts_with(EMOTE_EVIDENCE) {
        VoteSide::Accept
    } else if text.starts_with(EMOTE_NO_EVIDENCE) {
        VoteSide::Reject
    } else {
        return None;
    };
    let ids = text
        .split("<@")
        .skip(1)
        .filter_map(|s| s.split('>').next()?.parse().ok())
        .collect();
    Some((side, ids))
}

fn new_player_entry(username: String, tag_type: &str) -> PlayerEntry {
    PlayerEntry {
        username,
        uuid: String::new(),
        tag_type: tag_type.to_string(),
        reason: String::new(),
        status: PlayerStatus::Pending,
        reviewer: None,
        review_note: None,
        evidence: Vec::new(),
        conflict_warning: None,
        accept_votes: Vec::new(),
        reject_votes: Vec::new(),
    }
}

fn parse_status_line(text: &str) -> Option<(PlayerStatus, Option<String>, Option<String>)> {
    if text.starts_with("✅ Approved") {
        return Some((PlayerStatus::Approved, None, None));
    }
    if text.starts_with("❌ Rejected") {
        let note = text.find('"').and_then(|start| {
            let rest = &text[start + 1..];
            rest.find('"').map(|end| rest[..end].to_string())
        });
        return Some((PlayerStatus::Rejected, None, note));
    }
    None
}

pub fn lookup_tag_name_from_display(display: &str) -> Option<&'static str> {
    blacklist::all()
        .iter()
        .find(|t| t.display_name == display)
        .map(|t| t.name)
}

fn parse_evidence_line(line: &str) -> Option<Evidence> {
    let line = line.strip_prefix("- ").unwrap_or(line);
    if !line.starts_with("`/replay") {
        return None;
    }

    let command = line.split('`').nth(1)?;
    let replay = parse_replay(command)?;
    let note = line
        .split("Note: \"")
        .nth(1)
        .and_then(|s| s.strip_suffix('"'))
        .map(|s| s.to_string());
    Some(Evidence::Replay { replay, note })
}

pub fn render_replay_line(replay: &Replay, note: Option<&str>) -> String {
    match note {
        Some(n) => format!("- `{}` \u{2014} Note: \"{}\"", replay.format_command(), n),
        None => format!("- `{}`", replay.format_command()),
    }
}

pub fn render_evidence_summary(player: &PlayerEntry) -> Option<String> {
    let replays: Vec<String> = player
        .evidence
        .iter()
        .filter_map(|e| match e {
            Evidence::Replay { replay, note } => Some(render_replay_line(replay, note.as_deref())),
            _ => None,
        })
        .collect();

    if replays.is_empty() {
        return None;
    }
    Some(replays.join("\n"))
}

pub fn media_gallery_for(
    player: &PlayerEntry,
    existing_urls: &HashMap<String, String>,
) -> Option<CreateContainerComponent<'static>> {
    let items: Vec<CreateMediaGalleryItem> = player
        .evidence
        .iter()
        .filter_map(|e| match e {
            Evidence::Attachment { filename } => {
                let url = existing_urls
                    .get(filename)
                    .cloned()
                    .unwrap_or_else(|| format!("attachment://{filename}"));
                Some(CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(
                    url,
                )))
            }
            _ => None,
        })
        .collect();

    if items.is_empty() {
        return None;
    }
    Some(CreateContainerComponent::MediaGallery(
        CreateMediaGallery::new(items),
    ))
}

pub fn render_status_line(player: &PlayerEntry) -> String {
    match &player.status {
        PlayerStatus::Pending => "-# Pending review".to_string(),
        PlayerStatus::Approved => "✅ Approved".to_string(),
        PlayerStatus::Rejected => match &player.review_note {
            Some(note) => format!("❌ Rejected — \"{note}\""),
            None => "❌ Rejected".to_string(),
        },
    }
}

pub fn extract_media_urls_from_message(message: &Message, player_index: usize) -> Vec<String> {
    let Some(container) = find_container(message) else {
        return Vec::new();
    };

    let blocks = split_into_blocks(container);
    let mut idx = 0;
    for block in &blocks {
        if !block_has_player_marker(block) {
            continue;
        }
        if idx == player_index {
            return block
                .iter()
                .filter_map(|c| match c {
                    ContainerComponent::MediaGallery(gallery) => Some(
                        gallery
                            .items
                            .iter()
                            .map(|i| i.media.url.to_string())
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .flatten()
                .collect();
        }
        idx += 1;
    }
    Vec::new()
}

fn block_has_player_marker(block: &[&ContainerComponent]) -> bool {
    let scan = |content: &str| content.lines().any(|line| is_player_entry(line.trim()));
    block.iter().any(|c| match c {
        ContainerComponent::TextDisplay(td) => td.content.as_deref().is_some_and(scan),
        ContainerComponent::Section(section) => section.components.iter().any(|sc| match sc {
            SectionComponent::TextDisplay(td) => td.content.as_deref().is_some_and(scan),
            _ => false,
        }),
        _ => false,
    })
}

pub fn parse_confirmation_data(custom_id: &str, message: &Message) -> Option<ConfirmationData> {
    let stripped = custom_id.strip_prefix("review_confirm:")?;
    let parts: Vec<&str> = stripped.splitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }

    let tag_type = parts[1].to_string();
    let player_uuid = parts[2].to_string();

    let texts = extract_text_displays(message);
    let player_name = texts
        .iter()
        .find_map(|t| t.lines().find_map(|line| parse_player_ign(line.trim())))?;
    let reason = texts
        .iter()
        .find_map(|t| {
            t.lines()
                .find_map(|line| line.trim().strip_prefix("> ").map(|s| s.to_string()))
        })
        .unwrap_or_default();

    Some(ConfirmationData {
        player_name,
        player_uuid,
        tag_type,
        reason,
    })
}
