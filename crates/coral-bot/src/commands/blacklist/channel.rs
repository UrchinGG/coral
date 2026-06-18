use anyhow::Result;
use serenity::all::*;

use blacklist::{
    EMOTE_ADDTAG, EMOTE_EDITTAG, EMOTE_EVIDENCE, EMOTE_NO_EVIDENCE, EMOTE_REMOVETAG, EMOTE_TAG,
    lookup as lookup_tag,
};
use database::{BlacklistRepository, CacheRepository, GuildSubscriptionRepository, PlayerEvent};

use super::evidence::evidence_thread_url;

use crate::framework::{AccessRank, Data};
use crate::utils::{format_tag_detail, format_uuid_dashed, sanitize_reason};

const FACE_SIZE: u32 = 128;
const FACE_FILENAME: &str = "face.png";

pub const COLOR_ERROR: u32 = 0xED4245;

fn face_thumbnail() -> CreateThumbnail<'static> {
    CreateThumbnail::new(CreateUnfurledMediaItem::new(format!(
        "attachment://{FACE_FILENAME}"
    )))
}

async fn face_attachment(data: &Data, uuid: &str) -> CreateAttachment<'static> {
    let png = data
        .skin_provider
        .fetch_face(uuid, FACE_SIZE)
        .await
        .unwrap_or_else(|| {
            let img = image::RgbaImage::from_pixel(FACE_SIZE, FACE_SIZE, image::Rgba([0, 0, 0, 0]));
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
            buf.into_inner()
        });
    CreateAttachment::bytes(png, FACE_FILENAME)
}

fn face_section(parts: Vec<String>) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            parts.join("\n"),
        ))],
        CreateSectionAccessory::Thumbnail(face_thumbnail()),
    ))
}

pub async fn get_username(ctx: &Context, user_id: u64) -> String {
    ctx.http
        .get_user(UserId::new(user_id))
        .await
        .map(|u| u.name.to_string())
        .unwrap_or_else(|_| user_id.to_string())
}

pub async fn format_added_line(ctx: &Context, tag: &PlayerEvent) -> String {
    let ts = tag.ts.timestamp();
    match tag.author.filter(|_| !tag.hide_username.unwrap_or(false)) {
        Some(author) => {
            let username = get_username(ctx, author as u64).await;
            format!("> -# **\\- Added by `@{username}` <t:{ts}:R>**")
        }
        None => format!("> -# **\\- <t:{ts}:R>**"),
    }
}

pub fn tag_label(tag_type: &str) -> String {
    let def = lookup_tag(tag_type);
    let emote = def.map(|d| d.emote).unwrap_or("");
    let display = def.map(|d| d.display_name).unwrap_or(tag_type);
    format!("{emote} **{display}**")
}

pub fn evidence_indicator(tag_type: &str, has_evidence: bool) -> String {
    if tag_type != "confirmed_cheater" {
        return String::new();
    }
    let emote = if has_evidence {
        EMOTE_EVIDENCE
    } else {
        EMOTE_NO_EVIDENCE
    };
    format!(" {emote}")
}

const HISTORY_PER_PAGE: usize = 8;

fn reason_for_event(event: &PlayerEvent, all_events: &[PlayerEvent]) -> String {
    let reason = if event.kind == "tag_set" {
        event.reason.clone()
    } else {
        all_events
            .iter()
            .filter(|e| e.kind == "tag_set" && e.tag_type == event.tag_type && e.ts <= event.ts)
            .max_by_key(|e| e.ts)
            .and_then(|e| e.reason.clone())
    };
    sanitize_reason(reason.as_deref().unwrap_or(""))
}

fn render_history_entry(
    event: &PlayerEvent,
    all_events: &[PlayerEvent],
    names: &std::collections::HashMap<i64, String>,
) -> String {
    let (action_emote, action) = if event.kind == "tag_set" {
        (EMOTE_ADDTAG, "Added")
    } else {
        (EMOTE_REMOVETAG, "Removed")
    };
    let tag_type = event.tag_type.as_deref().unwrap_or("");
    let def = lookup_tag(tag_type);
    let emote = def.map(|d| d.emote).unwrap_or("");
    let display = def.map(|d| d.display_name).unwrap_or(tag_type);
    let ts = event.ts.timestamp();

    let attribution = match event.author {
        Some(id) => {
            let name = names.get(&id).cloned().unwrap_or_else(|| id.to_string());
            if event.hide_username.unwrap_or(false) {
                format!("**`@{name}`** (hidden)")
            } else {
                format!("**`@{name}`**")
            }
        }
        None => "**system**".to_string(),
    };

    let mut lines = vec![
        format!("**{action_emote} {action}**"),
        format!("> -# **{emote} {display}**"),
    ];
    let reason = reason_for_event(event, all_events);
    if !reason.is_empty() {
        lines.push(format!("> -# {reason}"));
    }
    lines.push(format!("> -# \\- {action} by {attribution} <t:{ts}:R>"));
    lines.join("\n")
}

async fn render_tag_history(
    ctx: &Context,
    username: &str,
    events: &[PlayerEvent],
    page: usize,
) -> (String, usize) {
    let header = format!("## Tag History — `{username}`");
    let mut tag_events: Vec<&PlayerEvent> = events
        .iter()
        .filter(|e| e.kind == "tag_set" || e.kind == "tag_clear")
        .collect();
    tag_events.sort_by(|a, b| b.ts.cmp(&a.ts).then(b.id.cmp(&a.id)));

    let total = tag_events.len();
    if total == 0 {
        return (format!("{header}\n-# No tag history yet"), 1);
    }
    let total_pages = total.div_ceil(HISTORY_PER_PAGE);
    let page = page.min(total_pages - 1);
    let slice = &tag_events[page * HISTORY_PER_PAGE..((page + 1) * HISTORY_PER_PAGE).min(total)];

    let mut names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for id in slice.iter().filter_map(|e| e.author) {
        if !names.contains_key(&id) {
            names.insert(id, get_username(ctx, id as u64).await);
        }
    }

    let mut out = vec![header];
    for event in slice {
        out.push(render_history_entry(event, events, &names));
    }
    (out.join("\n\n"), total_pages)
}

async fn respond_history(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    uuid: &str,
    page: usize,
    update: bool,
) -> Result<()> {
    let events = BlacklistRepository::new(data.db.pool())
        .get_tag_history(uuid)
        .await
        .unwrap_or_default();
    let username = CacheRepository::new(data.db.pool())
        .get_username(uuid)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| uuid.to_string());
    let (content, total_pages) = render_tag_history(ctx, &username, &events, page).await;
    let page = page.min(total_pages.saturating_sub(1));

    let mut parts = vec![CreateContainerComponent::TextDisplay(
        CreateTextDisplay::new(content),
    )];
    if total_pages > 1 {
        parts.push(CreateContainerComponent::TextDisplay(
            CreateTextDisplay::new(format!("-# Page {} / {}", page + 1, total_pages)),
        ));
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::buttons(vec![
                CreateButton::new(format!("tag_history_nav:{uuid}:{}", page.saturating_sub(1)))
                    .label("◀ Prev")
                    .style(ButtonStyle::Secondary)
                    .disabled(page == 0),
                CreateButton::new(format!("tag_history_nav:{uuid}:{}", page + 1))
                    .label("Next ▶")
                    .style(ButtonStyle::Secondary)
                    .disabled(page + 1 >= total_pages),
            ]),
        ));
    }

    let msg = CreateInteractionResponseMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            parts,
        ))]);
    let response = if update {
        CreateInteractionResponse::UpdateMessage(msg)
    } else {
        CreateInteractionResponse::Message(msg.ephemeral(true))
    };
    component.create_response(&ctx.http, response).await?;
    Ok(())
}

pub async fn handle_history_open(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let uuid = component
        .data
        .custom_id
        .strip_prefix("tag_history:")
        .unwrap_or_default();
    respond_history(ctx, component, data, uuid, 0, false).await
}

pub async fn handle_history_nav(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let rest = component
        .data
        .custom_id
        .strip_prefix("tag_history_nav:")
        .unwrap_or_default();
    let (uuid, page) = rest.rsplit_once(':').unwrap_or((rest, "0"));
    let page = page.parse().unwrap_or(0);
    respond_history(ctx, component, data, uuid, page, true).await
}

pub fn format_tag_block(
    tag_type: &str,
    detail: &str,
    evidence_indicator: &str,
    added_line: Option<&str>,
    reviewed_line: Option<&str>,
    strikethrough: bool,
) -> String {
    let def = lookup_tag(tag_type);
    let emote = def.map(|d| d.emote).unwrap_or("");
    let display = def.map(|d| d.display_name).unwrap_or(tag_type);

    let head = if strikethrough {
        format!("~~**{emote} {display}**~~{evidence_indicator}")
    } else {
        format!("**{emote} {display}**{evidence_indicator}")
    };

    let mut lines = vec![head, format!("> {detail}")];
    if let Some(a) = added_line {
        lines.push(a.to_string());
    }
    if let Some(r) = reviewed_line {
        lines.push(r.to_string());
    }
    lines.join("\n")
}

pub async fn format_reviewed_line(ctx: &Context, reviewed_by: Option<&[i64]>) -> Option<String> {
    let ids = reviewed_by.filter(|ids| !ids.is_empty())?;
    let names = futures::future::join_all(
        ids.iter()
            .map(|&id| async move { format!("`@{}`", get_username(ctx, id as u64).await) }),
    )
    .await;
    Some(format!("> -# **\\- Reviewed by {}**", names.join(", ")))
}

fn tag_footer(dashed_uuid: &str, review_url: Option<&str>, evidence_url: Option<&str>) -> String {
    let mut footer = format!("-# UUID: {dashed_uuid}");
    if let Some(url) = review_url {
        footer.push_str(&format!(" | [Review]({url})"));
    }
    if let Some(url) = evidence_url {
        footer.push_str(&format!(" | [Evidence]({url})"));
    }
    footer
}

#[allow(clippy::too_many_arguments)]
pub async fn post_new_tag(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    tag: &PlayerEvent,
    all_tags: &[PlayerEvent],
    silent: bool,
    review_url: Option<&str>,
) -> Option<MessageId> {
    post_tag_to_log(
        ctx,
        data,
        uuid,
        name,
        tag,
        "New Tag",
        EMOTE_ADDTAG,
        review_url,
    )
    .await;
    if silent {
        return None;
    }
    let watchers = guild_watchers(data, uuid, tag.tag_type.as_deref()).await;
    post_to_blacklist_channel(
        ctx,
        data,
        uuid,
        name,
        all_tags,
        "New Tag",
        EMOTE_ADDTAG,
        &watchers,
        review_url,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn post_overwritten_tag(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    tag: &PlayerEvent,
    all_tags: &[PlayerEvent],
    silent: bool,
    review_url: Option<&str>,
) -> Option<MessageId> {
    post_tag_to_log(
        ctx,
        data,
        uuid,
        name,
        tag,
        "Tag Overwritten",
        EMOTE_EDITTAG,
        review_url,
    )
    .await;
    if silent {
        return None;
    }
    let watchers = guild_watchers(data, uuid, tag.tag_type.as_deref()).await;
    post_to_blacklist_channel(
        ctx,
        data,
        uuid,
        name,
        all_tags,
        "Tag Overwritten",
        EMOTE_EDITTAG,
        &watchers,
        review_url,
    )
    .await
}

async fn guild_watchers(data: &Data, uuid: &str, tag_type: Option<&str>) -> Vec<UserId> {
    let Some(tag_type) = tag_type else {
        return Vec::new();
    };
    let Ok(Some(raw)) = data.api.get_guild_by_player(uuid).await else {
        return Vec::new();
    };
    let Some(guild_id) = raw["_id"].as_str() else {
        return Vec::new();
    };
    GuildSubscriptionRepository::new(data.db.pool())
        .subscribers_for(guild_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.tag_types.is_empty() || s.tag_types.iter().any(|t| t == tag_type))
        .map(|s| UserId::new(s.discord_id as u64))
        .collect()
}

pub async fn post_tag_removed(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    tag: &PlayerEvent,
    removed_by: u64,
    silent: bool,
) {
    let dashed_uuid = format_uuid_dashed(uuid);
    let added_line = format_added_line(ctx, tag).await;
    let username = get_username(ctx, removed_by).await;
    let detail = format_tag_detail(tag);

    let block = format_tag_block(
        tag.tag_type.as_deref().unwrap_or(""),
        &detail,
        "",
        Some(&added_line),
        None,
        true,
    );
    let log_footer = format!("-# Removed by `@{username}`\n-# UUID: {dashed_uuid}");
    let pub_footer = format!("-# UUID: {dashed_uuid}");

    let make_container = |footer: String| {
        CreateContainer::new(vec![
            face_section(vec![
                format!("## {} Tag Removed\nIGN - `{name}`\n", EMOTE_REMOVETAG),
                block.clone(),
                footer,
            ]),
            CreateContainerComponent::Separator(CreateSeparator::new(true)),
        ])
    };

    let log_face = face_attachment(data, uuid).await;
    send_to_mod_channel(
        ctx,
        data,
        make_container(log_footer).accent_color(COLOR_ERROR),
        vec![log_face],
    )
    .await;

    if silent {
        return;
    }
    let Some(channel_id) = data.blacklist_channel_id else {
        return;
    };
    let public_face = face_attachment(data, uuid).await;
    send_container(
        ctx,
        channel_id,
        make_container(pub_footer),
        vec![public_face],
        &[],
    )
    .await;
}

pub async fn post_tag_changed(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    old_tag: &PlayerEvent,
    new_tag: &PlayerEvent,
    title: &str,
    changed_by: u64,
) {
    let dashed_uuid = format_uuid_dashed(uuid);
    let old_added = format_added_line(ctx, old_tag).await;
    let new_added = format_added_line(ctx, new_tag).await;
    let username = get_username(ctx, changed_by).await;

    let old_block = format_tag_block(
        old_tag.tag_type.as_deref().unwrap_or(""),
        &format_tag_detail(old_tag),
        "",
        Some(&old_added),
        None,
        false,
    );
    let new_block = format_tag_block(
        new_tag.tag_type.as_deref().unwrap_or(""),
        &format_tag_detail(new_tag),
        "",
        Some(&new_added),
        None,
        false,
    );

    let face = face_attachment(data, uuid).await;
    let container = CreateContainer::new(vec![
        face_section(vec![
            format!("## {EMOTE_EDITTAG} {title}\nIGN - `{name}`\n"),
            format!("Previous:\n{old_block}"),
        ]),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!("New:\n{new_block}"))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "-# {title} by `@{username}`\n-# UUID: {dashed_uuid}"
        ))),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ]);

    send_to_mod_channel(ctx, data, container, vec![face]).await;
}

pub async fn post_lock_change(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    locked: bool,
    reason: Option<&str>,
    changed_by: u64,
) {
    let dashed_uuid = format_uuid_dashed(uuid);
    let title = if locked {
        format!("## {} Player Locked \u{1F512}\nIGN - `{}`", EMOTE_TAG, name)
    } else {
        format!(
            "## {} Player Unlocked \u{1F513}\nIGN - `{}`",
            EMOTE_TAG, name
        )
    };

    let face = face_attachment(data, uuid).await;
    let username = get_username(ctx, changed_by).await;
    let action = if locked { "Locked" } else { "Unlocked" };

    let mut section_parts = vec![title];
    if let Some(r) = reason {
        section_parts.push(format!("> {}", sanitize_reason(r)));
    }
    section_parts.push(format!(
        "-# {} by `@{}`\n-# UUID: {dashed_uuid}",
        action, username
    ));

    let parts = vec![
        face_section(section_parts),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ];

    send_to_mod_channel(ctx, data, CreateContainer::new(parts), vec![face]).await;
}

pub async fn post_key_locked(ctx: &Context, data: &Data, target_id: u64, invoker_id: u64) {
    post_key_change(ctx, data, target_id, invoker_id, true).await;
}

pub async fn post_key_unlocked(ctx: &Context, data: &Data, target_id: u64, invoker_id: u64) {
    post_key_change(ctx, data, target_id, invoker_id, false).await;
}

pub async fn post_access_changed(
    ctx: &Context,
    data: &Data,
    target_id: u64,
    old_rank: AccessRank,
    new_rank: AccessRank,
    invoker_id: u64,
) {
    let invoker = get_username(ctx, invoker_id).await;
    let container = CreateContainer::new(vec![
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "## Access Level Changed\n<@{target_id}>"
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "{} \u{2192} {}",
            old_rank.label(),
            new_rank.label()
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "-# Changed by `@{invoker}`"
        ))),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ]);

    send_to_mod_channel(ctx, data, container, vec![]).await;
}

pub async fn post_tagging_toggled(
    ctx: &Context,
    data: &Data,
    target_id: u64,
    disabled: bool,
    invoker_id: u64,
) {
    let title = if disabled {
        "Tagging Disabled"
    } else {
        "Tagging Enabled"
    };

    let invoker = get_username(ctx, invoker_id).await;
    let container = CreateContainer::new(vec![
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "## {title}\n<@{target_id}>"
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "-# Changed by `@{invoker}`"
        ))),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ]);
    let container = if disabled {
        container.accent_color(COLOR_ERROR)
    } else {
        container
    };

    send_to_mod_channel(ctx, data, container, vec![]).await;
}

async fn post_key_change(
    ctx: &Context,
    data: &Data,
    target_id: u64,
    invoker_id: u64,
    locked: bool,
) {
    let (title, action) = if locked {
        ("API Key Locked \u{1F512}", "Locked")
    } else {
        ("API Key Unlocked \u{1F513}", "Unlocked")
    };

    let invoker = get_username(ctx, invoker_id).await;
    let container = CreateContainer::new(vec![
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "## {title}\n<@{target_id}>"
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "-# {action} by `@{invoker}`"
        ))),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ]);
    let container = if locked {
        container.accent_color(COLOR_ERROR)
    } else {
        container
    };

    send_to_mod_channel(ctx, data, container, vec![]).await;
}

#[allow(clippy::too_many_arguments)]
async fn post_tag_to_log(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    tag: &PlayerEvent,
    title: &str,
    emote: &str,
    review_url: Option<&str>,
) {
    let dashed_uuid = format_uuid_dashed(uuid);
    let added_line = format_added_line(ctx, tag).await;
    let reviewed_line = format_reviewed_line(ctx, tag.reviewed_by.as_deref()).await;
    let evidence_url = evidence_thread_url(data, uuid);
    let face = face_attachment(data, uuid).await;

    let block = format_tag_block(
        tag.tag_type.as_deref().unwrap_or(""),
        &format_tag_detail(tag),
        "",
        Some(&added_line),
        reviewed_line.as_deref(),
        false,
    );

    let container = CreateContainer::new(vec![
        face_section(vec![
            format!("## {emote} {title}\nIGN - `{name}`\n"),
            block,
            tag_footer(&dashed_uuid, review_url, evidence_url.as_deref()),
        ]),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ]);

    send_to_mod_channel(ctx, data, container, vec![face]).await;
}

async fn send_to_mod_channel(
    ctx: &Context,
    data: &Data,
    container: CreateContainer<'static>,
    files: Vec<CreateAttachment<'static>>,
) {
    let Some(channel_id) = data.mod_channel_id else {
        return;
    };
    if send_container(ctx, channel_id, container, files, &[])
        .await
        .is_none()
    {
        tracing::warn!("Failed to post to mod channel {channel_id}");
    }
}

#[allow(clippy::too_many_arguments)]
async fn post_to_blacklist_channel(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    all_tags: &[PlayerEvent],
    title: &str,
    emote: &str,
    mentions: &[UserId],
    review_url: Option<&str>,
) -> Option<MessageId> {
    let channel_id = data.blacklist_channel_id?;
    let dashed_uuid = format_uuid_dashed(uuid);
    let evidence_url = evidence_thread_url(data, uuid);

    let face = face_attachment(data, uuid).await;

    let mut tag_texts = vec![];
    for tag in all_tags {
        let added_line = format_added_line(ctx, tag).await;
        let tag_type = tag.tag_type.as_deref().unwrap_or("");
        let indicator = evidence_indicator(tag_type, evidence_url.is_some());

        tag_texts.push(format_tag_block(
            tag_type,
            &format_tag_detail(tag),
            &indicator,
            Some(&added_line),
            None,
            false,
        ));
    }

    let footer = tag_footer(&dashed_uuid, review_url, evidence_url.as_deref());

    let header = format!("## {} {}\nIGN - `{}`\n", emote, title, name);
    let first_tag = tag_texts.first().cloned().unwrap_or_default();

    let mut parts = vec![face_section(vec![header, first_tag])];
    for tag_text in tag_texts.iter().skip(1) {
        parts.push(CreateContainerComponent::TextDisplay(
            CreateTextDisplay::new(tag_text.clone()),
        ));
    }
    parts.push(CreateContainerComponent::TextDisplay(
        CreateTextDisplay::new(footer),
    ));
    if !mentions.is_empty() {
        let pings = mentions
            .iter()
            .map(|u| format!("<@{}>", u.get()))
            .collect::<Vec<_>>()
            .join(" ");
        parts.push(CreateContainerComponent::TextDisplay(
            CreateTextDisplay::new(pings),
        ));
    }
    parts.push(CreateContainerComponent::Separator(CreateSeparator::new(
        true,
    )));

    send_container(
        ctx,
        channel_id,
        CreateContainer::new(parts),
        vec![face],
        mentions,
    )
    .await
}

async fn send_container(
    ctx: &Context,
    channel_id: ChannelId,
    container: CreateContainer<'static>,
    files: Vec<CreateAttachment<'static>>,
    mentions: &[UserId],
) -> Option<MessageId> {
    match ctx
        .http
        .send_message(
            channel_id.into(),
            files,
            &CreateMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(container)])
                .allowed_mentions(CreateAllowedMentions::new().users(mentions.to_vec())),
        )
        .await
    {
        Ok(msg) => Some(msg.id),
        Err(e) => {
            tracing::error!("Failed to post to channel {}: {}", channel_id, e);
            None
        }
    }
}
