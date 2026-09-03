use blacklist::{REPLAYS_NEEDED, lookup as lookup_tag};
use database::{AccountRepository, PendingTagNotice, PlayerEvent, TagNoticeRepository};
use serenity::all::*;

use crate::framework::Data;
use crate::utils::format_uuid_dashed;

const TICKET_URL: &str = "https://discord.com/channels/1339318572069158962/1342235914746986556";

/// Where the tag notice ended up, so the caller knows whether the blacklist
/// post still needs to ping the player.
pub enum NoticeDelivery {
    /// Nobody has this account linked, or the tag was not notifiable.
    NoOwner,
    /// The player got their DM.
    Dm,
    /// DMs are closed. Ping them on the blacklist post if there is one.
    PingInServer(UserId),
    /// DMs are closed and they are not in the server; a one-time notice is queued.
    Queued,
}

impl NoticeDelivery {
    pub fn ping(&self) -> Option<UserId> {
        match self {
            Self::PingInServer(id) => Some(*id),
            _ => None,
        }
    }
}

fn tag_label(tag_type: &str) -> String {
    lookup_tag(tag_type)
        .map(|d| d.display_name.to_string())
        .unwrap_or_else(|| tag_type.to_string())
}

fn reason_text(tag: &PlayerEvent) -> String {
    let reason = tag.reason.as_deref().unwrap_or("").trim();
    if reason.is_empty() {
        "No reason provided".to_string()
    } else {
        reason.replace('`', "'")
    }
}

fn appeal_body(user_id: UserId, username: &str, uuid: &str, detail: &str) -> String {
    format!(
        "Dear <@{user}>,\n\
         This message is to inform you that your account `{username}` (`{uuid}`) has been placed \
         on the Urchin Blacklist Network for `{detail}`.\n\n\
         If you believe this tag is false, please file an appeal immediately.\n\n\
         **How to File an Appeal**:\n\
         1. Access our official ticket system via Discord at {TICKET_URL}.\n\
         2. Select **Tag** from the menu.\n\
         3. Provide relevant details, documentation, or context supporting your request for \
         removal or adjustment.\n\n\
         We apologise for any disruption this notice may cause. Our goal remains the preservation \
         of accuracy of tags, and we appreciate your cooperation throughout this process.",
        user = user_id.get(),
    )
}

/// DMs the owner of `uuid` an appeal notice for a freshly applied tag, falling
/// back to a blacklist-post ping and/or a one-time in-command notice.
pub async fn notify_tagged_player(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    username: &str,
    tag: &PlayerEvent,
) -> NoticeDelivery {
    let Some(tag_type) = tag.tag_type.as_deref() else {
        return NoticeDelivery::NoOwner;
    };
    // Replays Needed is a request for footage, not an accusation to appeal.
    if tag_type == REPLAYS_NEEDED.name {
        return NoticeDelivery::NoOwner;
    }

    let pool = data.db.pool();
    let owner = match AccountRepository::new(pool).owner_discord_id(uuid).await {
        Ok(Some(id)) => id,
        Ok(None) => return NoticeDelivery::NoOwner,
        Err(e) => {
            tracing::error!("Failed to look up owner of {uuid} for tag notice: {e}");
            return NoticeDelivery::NoOwner;
        }
    };
    let user_id = UserId::new(owner as u64);

    let dashed_uuid = format_uuid_dashed(uuid);
    let detail = format!("{} - {}", tag_label(tag_type), reason_text(tag));

    if send_dm(
        ctx,
        user_id,
        &appeal_body(user_id, username, &dashed_uuid, &detail),
    )
    .await
    {
        return NoticeDelivery::Dm;
    }

    if let Err(e) = TagNoticeRepository::new(pool)
        .queue(owner, uuid, username, tag_type, &reason_text(tag))
        .await
    {
        tracing::error!("Failed to queue tag notice for {owner}: {e}");
    }

    if in_home_guild(ctx, data, user_id).await {
        NoticeDelivery::PingInServer(user_id)
    } else {
        NoticeDelivery::Queued
    }
}

async fn send_dm(ctx: &Context, user_id: UserId, content: &str) -> bool {
    let channel = match user_id.create_dm_channel(&ctx.http).await {
        Ok(channel) => channel,
        Err(e) => {
            tracing::debug!("Could not open DM channel with {user_id}: {e}");
            return false;
        }
    };
    match ctx
        .http
        .send_message(
            channel.id.into(),
            Vec::new(),
            &CreateMessage::new().content(content.to_string()),
        )
        .await
    {
        Ok(_) => true,
        Err(e) => {
            tracing::debug!("Could not DM tag notice to {user_id}: {e}");
            false
        }
    }
}

async fn in_home_guild(ctx: &Context, data: &Data, user_id: UserId) -> bool {
    let Some(guild_id) = data.home_guild_id else {
        return false;
    };
    ctx.http.get_member(guild_id, user_id).await.is_ok()
}

/// Ephemeral one-time notice for tags applied while the user was unreachable.
/// The caller must call [`mark_notice_delivered`] once the message lands so the
/// notice is only ever shown once.
pub async fn pending_notice(
    data: &Data,
    user_id: UserId,
) -> Option<(Vec<i64>, Vec<CreateComponent<'static>>)> {
    let notices = match TagNoticeRepository::new(data.db.pool())
        .list_pending(user_id.get() as i64)
        .await
    {
        Ok(notices) if !notices.is_empty() => notices,
        Ok(_) => return None,
        Err(e) => {
            tracing::error!("Failed to load tag notices for {user_id}: {e}");
            return None;
        }
    };
    let ids = notices.iter().map(|n| n.id).collect();
    Some((ids, notice_view(&notices)))
}

pub async fn mark_notice_delivered(data: &Data, ids: &[i64]) {
    if let Err(e) = TagNoticeRepository::new(data.db.pool())
        .mark_delivered(ids)
        .await
    {
        tracing::error!("Failed to mark tag notices {ids:?} delivered: {e}");
    }
}

fn notice_view(notices: &[PendingTagNotice]) -> Vec<CreateComponent<'static>> {
    let mut lines = vec![
        "## You were tagged".to_string(),
        "Since you last used the bot, the following account(s) of yours were placed on the \
         Urchin Blacklist Network."
            .to_string(),
    ];
    for notice in notices {
        lines.push(format!(
            "- `{}` (`{}`) - `{} - {}` <t:{}:R>",
            notice.username,
            format_uuid_dashed(&notice.uuid),
            tag_label(&notice.tag_type),
            notice.reason.replace('`', "'"),
            notice.created_at.timestamp(),
        ));
    }
    lines.push(format!(
        "If you believe a tag is false, file an appeal via our ticket system at {TICKET_URL}, \
         select **Tag** from the menu, and provide any supporting details."
    ));

    vec![CreateComponent::Container(CreateContainer::new(vec![
        crate::utils::text(lines.join("\n")),
    ]))]
}
