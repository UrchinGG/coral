use serenity::all::*;

use blacklist::{
    EMOTE_ADDTAG, EMOTE_EDITTAG, EMOTE_EVIDENCE, EMOTE_NO_EVIDENCE, EMOTE_REMOVETAG, EMOTE_TAG,
    lookup as lookup_tag,
};
use database::PlayerEvent;

use super::evidence::evidence_thread_url;

use crate::framework::{AccessRank, Data};
use crate::utils::{format_tag_detail, format_uuid_dashed, sanitize_reason};

const FACE_SIZE: u32 = 128;
const FACE_FILENAME: &str = "face.png";

pub const COLOR_SUCCESS: u32 = 0x00FF00;
pub const COLOR_DANGER: u32 = 0xFF5555;
pub const COLOR_ERROR: u32 = 0xED4245;
pub const COLOR_INFO: u32 = 0x5865F2;
pub const COLOR_FALLBACK: u32 = 0xFFA500;

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

pub async fn post_new_tag(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    tag: &PlayerEvent,
    all_tags: &[PlayerEvent],
) -> Option<MessageId> {
    post_tag_to_log(ctx, data, uuid, name, tag, "New Tag", EMOTE_ADDTAG).await;
    post_to_blacklist_channel(ctx, data, uuid, name, all_tags, "New Tag", EMOTE_ADDTAG).await
}

pub async fn post_overwritten_tag(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    tag: &PlayerEvent,
    all_tags: &[PlayerEvent],
) -> Option<MessageId> {
    post_tag_to_log(ctx, data, uuid, name, tag, "Tag Overwritten", EMOTE_EDITTAG).await;
    post_to_blacklist_channel(
        ctx,
        data,
        uuid,
        name,
        all_tags,
        "Tag Overwritten",
        EMOTE_EDITTAG,
    )
    .await
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
    let footer = format!("-# Removed by `@{username}`\n-# UUID: {dashed_uuid}");

    let make_container = || {
        CreateContainer::new(vec![
            face_section(vec![
                format!("## {} Tag Removed\nIGN - `{name}`\n", EMOTE_REMOVETAG),
                block.clone(),
                footer.clone(),
            ]),
            CreateContainerComponent::Separator(CreateSeparator::new(true)),
        ])
    };

    let log_face = face_attachment(data, uuid).await;
    send_to_mod_channel(
        ctx,
        data,
        make_container().accent_color(COLOR_DANGER),
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
    send_container(ctx, channel_id, make_container(), vec![public_face]).await;
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

pub async fn post_key_revoked(
    ctx: &Context,
    data: &Data,
    target_id: u64,
    reason: &str,
    invoker_id: u64,
) {
    let invoker = get_username(ctx, invoker_id).await;
    let container = CreateContainer::new(vec![
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "## \u{1F528} User Banned\n<@{target_id}>"
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "> {}",
            sanitize_reason(reason)
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "-# Banned by `@{invoker}`"
        ))),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ])
    .accent_color(COLOR_DANGER);

    send_to_mod_channel(ctx, data, container, vec![]).await;
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
        container.accent_color(COLOR_DANGER)
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
        container.accent_color(COLOR_DANGER)
    } else {
        container
    };

    send_to_mod_channel(ctx, data, container, vec![]).await;
}

async fn post_tag_to_log(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    tag: &PlayerEvent,
    title: &str,
    emote: &str,
) {
    let dashed_uuid = format_uuid_dashed(uuid);
    let added_line = format_added_line(ctx, tag).await;
    let face = face_attachment(data, uuid).await;

    let block = format_tag_block(
        tag.tag_type.as_deref().unwrap_or(""),
        &format_tag_detail(tag),
        "",
        Some(&added_line),
        None,
        false,
    );

    let container = CreateContainer::new(vec![
        face_section(vec![
            format!("## {emote} {title}\nIGN - `{name}`\n"),
            block,
            format!("-# UUID: {dashed_uuid}"),
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
    if send_container(ctx, channel_id, container, files)
        .await
        .is_none()
    {
        tracing::warn!("Failed to post to mod channel {channel_id}");
    }
}

async fn post_to_blacklist_channel(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    name: &str,
    all_tags: &[PlayerEvent],
    title: &str,
    emote: &str,
) -> Option<MessageId> {
    let channel_id = data.blacklist_channel_id?;
    let dashed_uuid = format_uuid_dashed(uuid);
    let evidence_url = evidence_thread_url(data, uuid);

    let face = face_attachment(data, uuid).await;

    let mut tag_texts = vec![];
    for tag in all_tags {
        let added_line = format_added_line(ctx, tag).await;
        let reviewed_line = match &tag.reviewed_by {
            Some(ids) if !ids.is_empty() => {
                let names: Vec<String> =
                    futures::future::join_all(ids.iter().map(|&id| async move {
                        format!("`@{}`", get_username(ctx, id as u64).await)
                    }))
                    .await;
                Some(format!("> -# **\\- Reviewed by {}**", names.join(", ")))
            }
            _ => None,
        };
        let tag_type = tag.tag_type.as_deref().unwrap_or("");
        let indicator = evidence_indicator(tag_type, evidence_url.is_some());

        tag_texts.push(format_tag_block(
            tag_type,
            &format_tag_detail(tag),
            &indicator,
            Some(&added_line),
            reviewed_line.as_deref(),
            false,
        ));
    }

    let mut footer = format!("-# UUID: {dashed_uuid}");
    if let Some(url) = &evidence_url {
        footer.push_str(&format!(" | [Evidence]({url})"));
    }

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
    parts.push(CreateContainerComponent::Separator(CreateSeparator::new(
        true,
    )));

    send_container(ctx, channel_id, CreateContainer::new(parts), vec![face]).await
}

async fn send_container(
    ctx: &Context,
    channel_id: ChannelId,
    container: CreateContainer<'static>,
    files: Vec<CreateAttachment<'static>>,
) -> Option<MessageId> {
    match ctx
        .http
        .send_message(
            channel_id.into(),
            files,
            &CreateMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(container)]),
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
