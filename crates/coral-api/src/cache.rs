use chrono::{Duration, Utc};
use database::{CacheRepository, GuildCacheRepository, GuildCurrentRepository};
use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

pub const SNAPSHOT_SOURCE: &str = "api";

const GUILD_REFRESH_AGE_MINUTES: i64 = 5;

pub fn parse_cache_age(raw: &str) -> Result<Duration, ApiError> {
    let invalid = || {
        ApiError::BadRequest(
            "max_cache_age must be like 30s, 5m, 1h, 2d, or a number of seconds".into(),
        )
    };
    let raw = raw.trim();
    if let Ok(secs) = raw.parse::<i64>() {
        return (secs >= 0)
            .then(|| Duration::seconds(secs))
            .ok_or_else(invalid);
    }
    let (value, unit) = raw.split_at(raw.len().checked_sub(1).ok_or_else(invalid)?);
    let n: i64 = value.parse().map_err(|_| invalid())?;
    if n < 0 {
        return Err(invalid());
    }
    match unit {
        "s" => Ok(Duration::seconds(n)),
        "m" => Ok(Duration::minutes(n)),
        "h" => Ok(Duration::hours(n)),
        "d" => Ok(Duration::days(n)),
        _ => Err(invalid()),
    }
}

pub fn spawn_guild_refresh(state: &AppState, uuid: &str) {
    let Some(hypixel) = state.hypixel.clone() else {
        return;
    };
    let state = state.clone();
    let uuid = uuid.to_string();
    tokio::spawn(async move {
        let cutoff = Utc::now() - Duration::minutes(GUILD_REFRESH_AGE_MINUTES);
        let Ok(guild_ids) = GuildCurrentRepository::new(state.db.pool())
            .stale_guilds_with_member(&uuid, cutoff)
            .await
        else {
            return;
        };
        for guild_id in guild_ids {
            if let Ok(Some(raw)) = hypixel.get_guild_by_id(&guild_id).await {
                cache_guild(&state, &raw).await;
            }
        }
    });
}

pub async fn cache_guild(state: &AppState, raw: &Value) {
    let Some(guild_id) = raw["_id"].as_str() else {
        return;
    };
    let pool = state.db.pool();
    let _ = GuildCacheRepository::new(pool)
        .store_snapshot(guild_id, raw)
        .await;
    let _ = GuildCurrentRepository::new(pool).upsert(raw).await;
    discover_members(state, raw).await;
}

async fn discover_members(state: &AppState, raw: &Value) {
    let Some(hypixel) = state.hypixel.as_deref() else {
        return;
    };
    let uuids: Vec<String> = raw["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["uuid"].as_str().map(String::from))
        .collect();
    let cache = CacheRepository::new(state.db.pool());
    let Ok(undiscovered) = cache.unregistered(&uuids).await else {
        return;
    };
    for uuid in undiscovered {
        if let Ok(Some(player)) = hypixel.get_player(&uuid).await {
            let username = player["displayname"].as_str();
            let _ = cache
                .store_snapshot(&uuid, &player, None, Some(SNAPSHOT_SOURCE), username)
                .await;
        }
    }
}

pub async fn refresh_player_cache(
    state: &AppState,
    uuid: &str,
    username: Option<&str>,
) -> Option<Value> {
    let hypixel = state.hypixel.as_deref()?;
    let data = hypixel.get_player(uuid).await.ok()??;
    let _ = CacheRepository::new(state.db.pool())
        .store_snapshot(uuid, &data, None, Some(SNAPSHOT_SOURCE), username)
        .await;
    Some(data)
}
