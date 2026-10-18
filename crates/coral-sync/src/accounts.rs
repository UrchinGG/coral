use anyhow::Result;
use serde_json::Value;
use serenity::all::*;

use database::{AccountRepository, LinkRole, MemberRepository};

use crate::framework::Data;

pub enum LinkCheck {
    Verified { uuid: String, username: String },
    NotVerified,
    PlayerNotFound,
    HypixelNotFound,
}

pub enum LinkOutcome {
    Added,
    AlreadyLinked,
}

pub async fn check_link(data: &Data, player: &str, discord_username: &str) -> LinkCheck {
    let stats = match data.api.get_player_stats(player).await {
        Ok(s) => s,
        Err(_) => return LinkCheck::PlayerNotFound,
    };
    let Some(hypixel_data) = stats.hypixel else {
        return LinkCheck::HypixelNotFound;
    };
    if is_discord_linked(&hypixel_data, discord_username) {
        LinkCheck::Verified {
            uuid: stats.uuid,
            username: stats.username,
        }
    } else {
        LinkCheck::NotVerified
    }
}

pub async fn link_primary(ctx: &Context, data: &Data, discord_id: u64, uuid: &str) -> Result<()> {
    let member = MemberRepository::new(data.db.pool())
        .create(discord_id as i64)
        .await?;
    let losers = AccountRepository::new(data.db.pool())
        .claim(member.id, uuid, LinkRole::Primary)
        .await?;
    resync(
        ctx,
        data,
        std::iter::once(discord_id).chain(losers.into_iter().map(|id| id as u64)),
    );
    Ok(())
}

pub async fn link_alt(
    ctx: &Context,
    data: &Data,
    discord_id: u64,
    member_id: i64,
    uuid: &str,
) -> Result<LinkOutcome> {
    let accounts = AccountRepository::new(data.db.pool());
    if accounts.is_owned_by(uuid, discord_id as i64).await? {
        return Ok(LinkOutcome::AlreadyLinked);
    }

    let member = MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id as i64)
        .await?;

    if member.as_ref().and_then(|m| m.uuid.as_ref()).is_none() {
        link_primary(ctx, data, discord_id, uuid).await?;
    } else {
        let losers = accounts.claim(member_id, uuid, LinkRole::Alt).await?;
        resync(ctx, data, losers.into_iter().map(|id| id as u64));
    }
    Ok(LinkOutcome::Added)
}

fn resync(ctx: &Context, data: &Data, discord_ids: impl Iterator<Item = u64>) {
    for discord_id in discord_ids {
        tokio::spawn(crate::sync::sync_user(
            ctx.clone(),
            data.clone(),
            UserId::new(discord_id),
        ));
    }
}

pub fn is_discord_linked(player: &Value, discord_username: &str) -> bool {
    player
        .get("socialMedia")
        .and_then(|s| s.get("links"))
        .and_then(|l| l.get("DISCORD"))
        .and_then(|d| d.as_str())
        .is_some_and(|linked| linked.to_lowercase() == discord_username.to_lowercase())
}
