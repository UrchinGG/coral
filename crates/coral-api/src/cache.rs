use chrono::Duration;
use database::CacheRepository;
use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

pub const SNAPSHOT_SOURCE: &str = "api";

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
