use blacklist::{REPLAYS_NEEDED, lookup as lookup_tag};
use database::{AccountRepository, PendingTagNotice, PlayerEvent, TagNoticeRepository};
use serenity::all::*;

use crate::framework::Data;
use crate::utils::format_uuid_dashed;

const TICKET_URL: &str = "https://discord.com/channels/1339318572069158962/1342235914746986556";

/// Where the tag notice ended up for each member who has the tagged account
/// linked, so the caller knows who still needs a ping on the blacklist post.
#[derive(Default)]
pub struct NoticeDelivery {
    /// The tag type does not warrant a notice (e.g. Replays Needed).
    not_notifiable: bool,
    /// Members who got their DM.
    dmed: Vec<UserId>,
    /// Members with DMs closed who are in the home guild; ping them instead.
    pinged: Vec<UserId>,
    /// Members with DMs closed who are not in the server; notice is queued.
    queued: Vec<UserId>,
}

impl NoticeDelivery {
    fn not_notifiable() -> Self {
        Self {
            not_notifiable: true,
            ..Self::default()
        }
    }

    /// Members to mention on the blacklist post because DMs did not reach them.
    pub fn pings(&self) -> &[UserId] {
        &self.pinged
    }

    fn no_owner(&self) -> bool {
        self.dmed.is_empty() && self.pinged.is_empty() && self.queued.is_empty()
    }

    /// Staff-log summary, so mods can see whether the tagged player heard about
    /// it and which accounts were reached.
    pub fn log_line(&self) -> String {
        if self.not_notifiable {
            return "-# Player notice: not sent for this tag type".to_string();
        }
        if self.no_owner() {
            return "-# Player notice: not registered with Urchin, nothing sent".to_string();
        }

        let mut lines = Vec::new();
        if !self.dmed.is_empty() {
            lines.push(format!(
                "-# Player notice: DM delivered to {}",
                mentions(&self.dmed)
            ));
        }
        if !self.pinged.is_empty() {
            lines.push(format!(
                "-# Player notice: DMs closed for {}, pinged in server and queued for next command",
                mentions(&self.pinged)
            ));
        }
        if !self.queued.is_empty() {
            lines.push(format!(
                "-# Player notice: DMs closed for {}, queued for next command",
                mentions(&self.queued)
            ));
        }
        lines.join("\n")
    }
}

fn mentions(ids: &[UserId]) -> String {
    ids.iter()
        .map(|id| format!("<@{id}>"))
        .collect::<Vec<_>>()
        .join(", ")
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

/// DMs every member who has `uuid` linked an appeal notice for a freshly
/// applied tag, falling back to a blacklist-post ping and/or a one-time
/// in-command notice for anyone whose DMs are closed.
pub async fn notify_tagged_player(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    username: &str,
    tag: &PlayerEvent,
) -> NoticeDelivery {
    let Some(tag_type) = tag.tag_type.as_deref() else {
        return NoticeDelivery::not_notifiable();
    };
    // Replays Needed is a request for footage, not an accusation to appeal.
    if tag_type == REPLAYS_NEEDED.name {
        return NoticeDelivery::not_notifiable();
    }

    let pool = data.db.pool();
    let owners = match AccountRepository::new(pool).owner_discord_ids(uuid).await {
        Ok(owners) => owners,
        Err(e) => {
            tracing::error!("Failed to look up owners of {uuid} for tag notice: {e}");
            Vec::new()
        }
    };

    let dashed_uuid = format_uuid_dashed(uuid);
    let reason = reason_text(tag);
    let detail = format!("{} - {}", tag_label(tag_type), reason);

    let mut delivery = NoticeDelivery::default();
    for owner in owners {
        let user_id = UserId::new(owner as u64);

        if send_dm(
            ctx,
            user_id,
            &appeal_body(user_id, username, &dashed_uuid, &detail),
        )
        .await
        {
            delivery.dmed.push(user_id);
            continue;
        }

        if let Err(e) = TagNoticeRepository::new(pool)
            .queue(owner, uuid, username, tag_type, &reason)
            .await
        {
            tracing::error!("Failed to queue tag notice for {owner}: {e}");
        }

        if in_home_guild(ctx, data, user_id).await {
            delivery.pinged.push(user_id);
        } else {
            delivery.queued.push(user_id);
        }
    }
    delivery
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
