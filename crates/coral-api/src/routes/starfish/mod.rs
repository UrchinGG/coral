pub mod auth;
mod download;
mod license;
pub mod plugins;
pub mod session_auth;
mod users;

use std::sync::Arc;

use axum::Router;
use coral_redis::RateLimitResult;

use crate::{
    error::ApiError,
    state::{AppState, StarfishConfig},
};

pub const MOUNT_PREFIX: &str = "/api/v1/starfish";

pub(crate) fn require_starfish(state: &AppState) -> Result<Arc<StarfishConfig>, ApiError> {
    state
        .starfish
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("Starfish not configured".into()))
}

pub(crate) async fn rate_limit(state: &AppState, key: &str, limit: i64) -> Result<(), ApiError> {
    match state.rate_limiter.check_and_record(key, limit).await {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(ApiError::RateLimited),
        Err(_) => Ok(()),
    }
}

pub(crate) fn is_owner(discord_id: i64) -> bool {
    static OWNERS: std::sync::OnceLock<std::collections::HashSet<i64>> = std::sync::OnceLock::new();
    OWNERS
        .get_or_init(|| {
            std::env::var("OWNER_IDS")
                .unwrap_or_default()
                .split(',')
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect()
        })
        .contains(&discord_id)
}

pub(crate) fn require_owner(
    caller: &session_auth::AuthenticatedStarfishUser,
) -> Result<(), ApiError> {
    if is_owner(caller.user.discord_id) {
        Ok(())
    } else {
        Err(ApiError::Forbidden("owner_only".into()))
    }
}

pub(crate) async fn resolve_discord_user(
    state: &AppState,
    bearer_token: &str,
) -> Result<Option<database::starfish::StarfishUser>, ApiError> {
    let discord_user = auth::fetch_discord_user(bearer_token).await?;
    let discord_id: i64 = discord_user
        .id
        .parse()
        .map_err(|_| ApiError::Internal("Invalid Discord ID".into()))?;

    database::StarfishRepository::new(state.db.pool())
        .get_user_by_discord_id(discord_id)
        .await
        .map_err(Into::into)
}

pub fn router(state: AppState) -> Router<AppState> {
    if state.starfish.is_none() {
        tracing::info!("Starfish routes disabled (no config)");
        return Router::new();
    }

    Router::new()
        .merge(auth::router())
        .merge(license::router())
        .merge(download::router())
        .merge(users::router(state.clone()))
        .merge(plugins::router(state))
}
