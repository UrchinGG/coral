use chrono::{DateTime, Duration, Utc};
use coral_redis::minute_history::{CHANGED_SNAPSHOTS, FIRST_SNAPSHOTS, SNAPSHOT_HISTORY};
use database::{CacheRepository, GuildCacheRepository, GuildCurrentRepository, SnapshotResult};
use redis::aio::ConnectionManager;
use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

const SNAPSHOT_SOURCE: &str = "api";

const GUILD_REFRESH_AGE_MINUTES: i64 = 5;
const BACKGROUND_FRESH_MINUTES: i64 = 10;
pub const DEFAULT_BACKGROUND_REFRESH_BUDGET: i64 = 90;
const BACKGROUND_BUDGET_NAME: &str = "hypixel:background";

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
            if !take_background_budget(&state).await {
                return;
            }
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
        if !take_background_budget(state).await {
            return;
        }
        if let Ok(Some(player)) = hypixel.get_player(&uuid).await {
            store_player_snapshot(state, &uuid, &player, player["displayname"].as_str()).await;
        }
    }
}

pub fn spawn_background_refresh(state: &AppState, uuid: &str) {
    let (state, uuid) = (state.clone(), uuid.to_string());
    tokio::spawn(async move {
        if is_fresh(&state, &uuid).await || !take_background_budget(&state).await {
            return;
        }
        refresh_player_cache(&state, &uuid, None).await;
    });
}

pub async fn refresh_player_cache(
    state: &AppState,
    uuid: &str,
    username: Option<&str>,
) -> Option<Value> {
    let hypixel = state.hypixel.as_deref()?;
    let data = hypixel.get_player(uuid).await.ok()??;
    store_player_snapshot(state, uuid, &data, username).await;
    Some(data)
}

pub async fn store_player_snapshot(
    state: &AppState,
    uuid: &str,
    data: &Value,
    username: Option<&str>,
) {
    let stored = CacheRepository::new(state.db.pool())
        .store_snapshot(uuid, data, None, Some(SNAPSHOT_SOURCE), username)
        .await;
    let stored = match stored {
        Ok(stored) => stored,
        Err(e) => {
            tracing::warn!("failed to store a snapshot of {uuid}: {e}");
            return;
        }
    };
    let mut redis = state.redis.connection();
    if let Err(e) = count_snapshot(&mut redis, &stored, Utc::now()).await {
        tracing::warn!("failed to count a stored snapshot of {uuid}: {e}");
    }
}

async fn is_fresh(state: &AppState, uuid: &str) -> bool {
    let cutoff = Utc::now() - Duration::minutes(BACKGROUND_FRESH_MINUTES);
    matches!(
        CacheRepository::new(state.db.pool())
            .get_latest_timestamp(uuid)
            .await,
        Ok(Some(ts)) if ts > cutoff
    )
}

async fn count_snapshot(
    redis: &mut ConnectionManager,
    stored: &SnapshotResult,
    at: DateTime<Utc>,
) -> Result<(), redis::RedisError> {
    let Some(counter) = snapshot_counter(stored) else {
        return Ok(());
    };
    SNAPSHOT_HISTORY.record(redis, at, &[counter]).await
}

fn snapshot_counter(stored: &SnapshotResult) -> Option<&'static str> {
    match stored {
        SnapshotResult::Stored { first: true, .. } => Some(FIRST_SNAPSHOTS),
        SnapshotResult::Stored { first: false, .. } => Some(CHANGED_SNAPSHOTS),
        SnapshotResult::NoChange => None,
    }
}

async fn take_background_budget(state: &AppState) -> bool {
    match state
        .rate_limiter
        .try_acquire(BACKGROUND_BUDGET_NAME, state.background_refresh_budget)
        .await
    {
        Ok(granted) => granted,
        Err(e) => {
            tracing::warn!("Background Hypixel budget unavailable: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use redis::{AsyncCommands, Client};
    use serde_json::json;
    use sqlx::PgPool;

    use super::*;

    const SAMPLE_INSTANT: &str = "1970-01-04T00:09:00Z";

    async fn test_stores() -> Option<(PgPool, ConnectionManager)> {
        dotenvy::dotenv().ok();
        let pool = PgPool::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?;
        let client = Client::open(std::env::var("REDIS_URL").ok()?).ok()?;
        let redis = ConnectionManager::new(client).await.ok()?;
        Some((pool, redis))
    }

    async fn forget_player(pool: &PgPool, uuid: &str) {
        for statement in [
            "DELETE FROM player_snapshots WHERE uuid = $1",
            "DELETE FROM players WHERE uuid = $1",
        ] {
            sqlx::query(statement)
                .bind(uuid)
                .execute(pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn stored_snapshots_count_as_first_or_changed_and_unchanged_ones_do_not() {
        let Some((pool, mut redis)) = test_stores().await else {
            return;
        };
        let at: DateTime<Utc> = SAMPLE_INSTANT.parse().unwrap();
        let slot = SNAPSHOT_HISTORY.slot(at);
        let () = redis.del(&slot.key).await.unwrap();
        let uuid = format!("{:032x}", Utc::now().timestamp_nanos_opt().unwrap());
        let repo = CacheRepository::new(&pool);

        for data in [
            json!({"kills": 1}),
            json!({"kills": 2}),
            json!({"kills": 2}),
        ] {
            let stored = repo
                .store_snapshot(&uuid, &data, None, Some(SNAPSHOT_SOURCE), None)
                .await
                .unwrap();
            count_snapshot(&mut redis, &stored, at).await.unwrap();
        }

        let counts: HashMap<String, i64> = redis.hgetall(&slot.key).await.unwrap();
        let () = redis.del(&slot.key).await.unwrap();
        forget_player(&pool, &uuid).await;
        assert_eq!(
            counts,
            HashMap::from([("f9".to_string(), 1), ("d9".to_string(), 1)])
        );
    }
}
