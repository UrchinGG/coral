use anyhow::Result;
use serde_json::Value;

use coral_redis::{LinkMethod, SyncEvent};
use database::{AccountRepository, IdentityRepository, MemberRepository};

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

pub async fn link_primary(
    data: &Data,
    discord_id: u64,
    uuid: &str,
    method: LinkMethod,
) -> Result<()> {
    let repo = MemberRepository::new(data.db.pool());
    let member = repo.create(discord_id as i64).await?;
    repo.set_uuid(discord_id as i64, uuid).await?;
    claim(data, discord_id, member.id, uuid, method).await?;
    data.sync_event_publisher
        .publish(&SyncEvent::SyncUser { discord_id })
        .await;
    Ok(())
}

pub async fn link_alt(
    data: &Data,
    discord_id: u64,
    member_id: i64,
    uuid: &str,
    method: LinkMethod,
) -> Result<LinkOutcome> {
    let member = MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id as i64)
        .await?;
    let primary = member.as_ref().and_then(|m| m.uuid.as_deref());

    if primary == Some(uuid) {
        return Ok(if claim(data, discord_id, member_id, uuid, method).await? {
            data.sync_event_publisher
                .publish(&SyncEvent::SyncUser { discord_id })
                .await;
            LinkOutcome::Added
        } else {
            LinkOutcome::AlreadyLinked
        });
    }

    let accounts = AccountRepository::new(data.db.pool());
    if accounts.is_owned_by(uuid, discord_id as i64).await? {
        return Ok(LinkOutcome::AlreadyLinked);
    }

    if primary.is_none() {
        link_primary(data, discord_id, uuid, method).await?;
    } else {
        accounts.add(member_id, uuid).await?;
    }
    Ok(LinkOutcome::Added)
}

async fn claim(
    data: &Data,
    discord_id: u64,
    member_id: i64,
    uuid: &str,
    method: LinkMethod,
) -> Result<bool> {
    let previous = IdentityRepository::new(data.db.pool())
        .claim(member_id, uuid)
        .await?;
    let Some(previous) = previous else {
        return Ok(false);
    };
    data.sync_event_publisher
        .publish(&SyncEvent::IdentityTakeover {
            uuid: uuid.to_string(),
            previous_discord_id: previous as u64,
            new_discord_id: discord_id,
            method,
        })
        .await;
    Ok(true)
}

pub fn is_discord_linked(player: &Value, discord_username: &str) -> bool {
    player
        .get("socialMedia")
        .and_then(|s| s.get("links"))
        .and_then(|l| l.get("DISCORD"))
        .and_then(|d| d.as_str())
        .is_some_and(|linked| linked.to_lowercase() == discord_username.to_lowercase())
}
