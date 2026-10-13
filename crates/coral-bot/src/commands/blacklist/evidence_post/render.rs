use blacklist::{EMOTE_EVIDENCE, EMOTE_NO_EVIDENCE};
use serenity::all::*;

use database::{BlacklistRepository, EvidencePost};

use super::super::channel::{format_added_line, format_reviewed_line, format_tag_block};
use super::FACE_FILENAME;
use crate::framework::Data;
use crate::utils::{format_uuid_dashed, sanitize_reason, separator, text};

const REVIEW_TAGS: &[&str] = &["confirmed_cheater", "blatant_cheater", "closet_cheater"];

pub struct TagView {
    pub reason: String,
    pub added_line: Option<String>,
    pub reviewed_line: Option<String>,
}

pub async fn tag_view(ctx: &Context, data: &Data, post: &EvidencePost) -> anyhow::Result<TagView> {
    let tags = BlacklistRepository::new(data.db.pool())
        .get_active_tags(&post.uuid)
        .await?;
    let tag = REVIEW_TAGS
        .iter()
        .find_map(|wanted| tags.iter().find(|t| t.tag_type.as_deref() == Some(*wanted)));
    if let Some(tag) = tag {
        return Ok(TagView {
            reason: tag.reason.clone().unwrap_or_default(),
            added_line: Some(format_added_line(ctx, tag).await),
            reviewed_line: format_reviewed_line(ctx, tag.reviewed_by.as_deref()).await,
        });
    }
    if let Some(pending) = &post.pending {
        let name = super::super::channel::get_username(ctx, pending.by as u64).await;
        return Ok(TagView {
            reason: pending.reason.clone(),
            added_line: Some(format!("> -# **\\- Added by `@{name}`**")),
            reviewed_line: None,
        });
    }
    Ok(TagView {
        reason: String::new(),
        added_line: None,
        reviewed_line: None,
    })
}

pub fn live(
    username: &str,
    uuid: &str,
    tag: &TagView,
    files: &[String],
) -> Vec<CreateComponent<'static>> {
    let block = format_tag_block(
        "confirmed_cheater",
        &sanitize_reason(&tag.reason),
        "",
        tag.added_line.as_deref(),
        tag.reviewed_line.as_deref(),
        false,
    );
    let mut parts = vec![text(format!(
        "## {} Evidence — `{username}`",
        header_emote(files)
    ))];
    match gallery(files) {
        Some(gallery) => parts.push(gallery),
        None => parts.push(text("-# No evidence added yet")),
    }
    parts.push(face_section(format!(
        "{block}\n-# UUID: {}",
        format_uuid_dashed(uuid)
    )));
    parts.push(history_row(uuid));
    parts.push(separator());
    let mut buttons = vec![
        CreateButton::new("evidence_add_media")
            .label("Add Media")
            .style(ButtonStyle::Primary),
    ];
    if !files.is_empty() {
        buttons.push(
            CreateButton::new(format!("evidence_manage:{uuid}"))
                .label("Remove Media")
                .style(ButtonStyle::Secondary),
        );
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(buttons),
    ));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

pub fn archived(username: &str, uuid: &str, files: &[String]) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![face_section(format!(
        "## {} Evidence — `{username}` (Archived)\n-# UUID: {}",
        header_emote(files),
        format_uuid_dashed(uuid)
    ))];
    if let Some(gallery) = gallery(files) {
        parts.push(gallery);
    }
    parts.push(history_row(uuid));
    parts.push(separator());
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

pub fn face_section(content: String) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            content,
        ))],
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
            format!("attachment://{FACE_FILENAME}"),
        ))),
    ))
}

fn header_emote(files: &[String]) -> &'static str {
    if files.is_empty() {
        EMOTE_NO_EVIDENCE
    } else {
        EMOTE_EVIDENCE
    }
}

fn gallery(files: &[String]) -> Option<CreateContainerComponent<'static>> {
    let items: Vec<CreateMediaGalleryItem> = files
        .iter()
        .map(|f| {
            CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(format!("attachment://{f}")))
        })
        .collect();
    (!items.is_empty())
        .then(|| CreateContainerComponent::MediaGallery(CreateMediaGallery::new(items)))
}

fn history_row(uuid: &str) -> CreateContainerComponent<'static> {
    CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
        CreateButton::new(format!("tag_history:{uuid}"))
            .label("Tag History")
            .style(ButtonStyle::Secondary),
    ]))
}

pub fn notice(
    emote_title: &str,
    username: &str,
    thread_link: Option<&str>,
    lines: &[&str],
) -> Vec<CreateComponent<'static>> {
    let mut body = format!("## {emote_title}\nIGN - `{username}`");
    if let Some(link) = thread_link {
        body.push_str(&format!("\nThread: {link}"));
    }
    let mut parts = vec![face_section(body)];
    for line in lines {
        parts.push(text(format!("-# {line}")));
    }
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}
