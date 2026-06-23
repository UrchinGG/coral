use std::collections::HashMap;
use std::io::Cursor;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use clients::{is_uuid, normalize_uuid};
use database::{BlacklistRepository, CacheRepository, permissions};
use hypixel::extract_rank_prefix;

use crate::{
    auth::DeveloperKeyAuth,
    cache::{SNAPSHOT_SOURCE, parse_cache_age, refresh_player_cache},
    error::{ApiError, ErrorResponse},
    responses::{PlayerStatsResponse, PlayerTagsResponse, tag_responses},
    state::AppState,
};

#[derive(Deserialize)]
pub(crate) struct PlayerQuery {
    pub player: Option<String>,
    pub uuid: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub max_cache_age: Option<String>,
}

pub(crate) async fn resolve_player_data(
    state: &AppState,
    uuid: &str,
    max_cache_age: Option<Duration>,
) -> Result<(Option<Value>, bool), ApiError> {
    crate::cache::spawn_guild_refresh(state, uuid);
    let cache = CacheRepository::new(state.db.pool());

    if let Some(max_age) = max_cache_age {
        if let Some(ts) = cache.get_latest_timestamp(uuid).await? {
            if Utc::now() - ts <= max_age {
                if let Some(snapshot) = cache.get_latest_snapshot(uuid).await? {
                    return Ok((Some(snapshot), true));
                }
            }
        }
    }

    match state.require_hypixel()?.get_player(uuid).await {
        Ok(data) => Ok((data, false)),
        Err(err) if max_cache_age.is_some() => match cache.get_latest_snapshot(uuid).await? {
            Some(snapshot) => Ok((Some(snapshot), true)),
            None => Err(err.into()),
        },
        Err(err) => Err(err.into()),
    }
}

pub fn public_router() -> Router<AppState> {
    Router::new().route("/player/tags", get(player_tags))
}

#[derive(Deserialize)]
pub(crate) struct FaceQuery {
    pub player: Option<String>,
    pub uuid: Option<String>,
    pub name: Option<String>,
    pub size: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct BodyQuery {
    pub player: Option<String>,
    pub uuid: Option<String>,
    pub name: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub fn internal_router() -> Router<AppState> {
    Router::new()
        .route("/player/profile", get(player_stats))
        .route("/player/skin", get(player_skin))
        .route("/player/face", get(player_face))
        .route("/player/body", get(player_body))
}

pub async fn resolve_identifier(
    state: &AppState,
    identifier: &str,
) -> Result<(String, Option<String>), ApiError> {
    if is_uuid(identifier) {
        Ok((normalize_uuid(identifier), None))
    } else {
        let id = state.mojang.resolve(identifier).await?;
        Ok((normalize_uuid(&id.uuid), Some(id.username)))
    }
}

pub(crate) fn pick_identifier<'a>(
    player: Option<&'a str>,
    uuid: Option<&'a str>,
    name: Option<&'a str>,
) -> Result<&'a str, ApiError> {
    player
        .or(uuid)
        .or(name)
        .ok_or_else(|| ApiError::BadRequest("query parameter 'player' required".into()))
}

pub(crate) trait PlayerTarget {
    fn identifier(&self) -> Result<&str, ApiError>;
}

macro_rules! player_target {
    ($($t:ty),+ $(,)?) => {$(
        impl PlayerTarget for $t {
            fn identifier(&self) -> Result<&str, ApiError> {
                pick_identifier(self.player.as_deref(), self.uuid.as_deref(), self.name.as_deref())
            }
        }
    )+};
}

player_target!(PlayerQuery, FaceQuery, BodyQuery);

pub(crate) fn format_display_name(player: &Value) -> Option<String> {
    let name = player.get("displayname").and_then(Value::as_str)?;
    Some(format!(
        "{}{name}",
        extract_rank_prefix(player).unwrap_or_default()
    ))
}

pub(crate) async fn player_display_name(state: &AppState, uuid: &str) -> Option<String> {
    refresh_player_cache(state, uuid, None)
        .await
        .as_ref()
        .and_then(format_display_name)
}

pub(crate) async fn cached_display_name(state: &AppState, uuid: &str) -> Option<String> {
    CacheRepository::new(state.db.pool())
        .get_latest_snapshot(uuid)
        .await
        .ok()
        .flatten()
        .as_ref()
        .and_then(format_display_name)
}

fn resolve_username(hint: Option<String>, player_data: &Option<Value>, uuid: &str) -> String {
    hint.unwrap_or_else(|| {
        player_data
            .as_ref()
            .and_then(|d| d["displayname"].as_str())
            .map(String::from)
            .unwrap_or_else(|| uuid.to_string())
    })
}

fn spawn_cache_update(state: &AppState, uuid: &str, data: &Value, username: &str) {
    let (pool, uuid, data, username) = (
        state.db.pool().clone(),
        uuid.to_string(),
        data.clone(),
        username.to_string(),
    );
    tokio::spawn(async move {
        let _ = CacheRepository::new(&pool)
            .store_snapshot(&uuid, &data, None, Some(SNAPSHOT_SOURCE), Some(&username))
            .await;
    });
}

#[utoipa::path(
    get,
    path = "/v3/player/tags",
    description = "Returns the blacklist tags currently active on a player, plus their formatted Hypixel display name.",
    params(
        ("player" = String, Query, description = "Player identifier: username, dashed UUID, or undashed UUID"),
    ),
    responses(
        (status = 200, description = "Player tags retrieved", body = PlayerTagsResponse),
        (status = 400, description = "Invalid identifier", body = ErrorResponse),
        (status = 404, description = "Player not found", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 502, description = "External API error", body = ErrorResponse),
    ),
    tag = "Player",
)]
pub async fn player_tags(
    State(state): State<AppState>,
    Query(query): Query<PlayerQuery>,
) -> Result<Json<PlayerTagsResponse>, ApiError> {
    let identifier = query.identifier()?;
    let (uuid, _) = resolve_identifier(&state, identifier).await?;
    let tags = BlacklistRepository::new(state.db.pool())
        .get_active_tags(&uuid)
        .await?;
    let tags = tag_responses(&tags, &state.discord, &mut HashMap::new()).await;
    let displayname = player_display_name(&state, &uuid).await;
    Ok(Json(PlayerTagsResponse {
        uuid,
        displayname,
        tags,
    }))
}

#[utoipa::path(
    get,
    path = "/v3/player/profile",
    description = "Returns a player's full Hypixel profile, including their blacklist tags and skin metadata. Set `max_cache_age` (for example `5m`, `1h`, or a number of seconds) to serve a stored snapshot within that age instead of calling Hypixel, and to fall back to the latest snapshot when Hypixel is unreachable. Responses served from a snapshot are marked `stale`. Requires the `Player Data` permission or an Admin key.",
    params(
        ("player" = String, Query, description = "Player identifier: username, dashed UUID, or undashed UUID"),
        ("max_cache_age" = Option<String>, Query, description = "Accept a stored snapshot up to this age (e.g. `5m`, `1h`, `30s`)"),
    ),
    responses(
        (status = 200, description = "Player profile retrieved", body = PlayerStatsResponse),
        (status = 400, description = "Invalid identifier", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Player not found", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 502, description = "External API error", body = ErrorResponse),
    ),
    tag = "Internal",
    security(("api_key" = []))
)]
pub async fn player_stats(
    State(state): State<AppState>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<PlayerQuery>,
) -> Result<Json<PlayerStatsResponse>, ApiError> {
    if let Some(Extension(ref dev)) = dev_auth {
        dev.require(permissions::PLAYER_DATA)?;
    }
    let max_cache_age = query
        .max_cache_age
        .as_deref()
        .map(parse_cache_age)
        .transpose()?;
    let identifier = query.identifier()?;
    let (uuid, username_hint) = resolve_identifier(&state, identifier).await?;
    let repo = BlacklistRepository::new(state.db.pool());
    let (player_result, tags, profile) = tokio::join!(
        resolve_player_data(&state, &uuid, max_cache_age),
        repo.get_active_tags(&uuid),
        state.mojang.get_profile(&uuid),
    );
    let (player_data, stale) = player_result?;
    let tags = tags?;
    let (skin_url, slim) = match profile.ok() {
        Some(p) => (p.skin_url, p.slim),
        None => (None, false),
    };

    let username = resolve_username(username_hint, &player_data, &uuid);

    if !stale {
        if let Some(ref data) = player_data {
            spawn_cache_update(&state, &uuid, data, &username);
        }
    }

    let displayname = player_data.as_ref().and_then(format_display_name);
    let tags = tag_responses(&tags, &state.discord, &mut HashMap::new()).await;
    Ok(Json(PlayerStatsResponse {
        uuid,
        username,
        displayname,
        hypixel: player_data,
        tags,
        skin_url,
        slim,
        stale,
    }))
}

#[utoipa::path(
    get,
    path = "/v3/player/skin",
    description = "Renders a player's full skin as a PNG. Requires the `Player Data` permission or an Admin key.",
    params(
        ("player" = String, Query, description = "Player identifier: username, dashed UUID, or undashed UUID"),
    ),
    responses(
        (status = 200, description = "Player skin PNG", content_type = "image/png"),
        (status = 400, description = "Invalid identifier", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Skin not found", body = ErrorResponse),
        (status = 500, description = "Skin rendering unavailable", body = ErrorResponse),
    ),
    tag = "Internal",
    security(("api_key" = []))
)]
pub async fn player_skin(
    State(state): State<AppState>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<PlayerQuery>,
) -> Result<Response, ApiError> {
    if let Some(Extension(ref dev)) = dev_auth {
        dev.require(permissions::PLAYER_DATA)?;
    }
    let provider = state
        .skin_provider
        .as_ref()
        .ok_or_else(|| ApiError::Internal("skin rendering unavailable".into()))?;
    let identifier = query.identifier()?;
    let (uuid, _) = resolve_identifier(&state, identifier).await?;
    let skin = provider
        .fetch(&uuid, 400, 600)
        .await
        .ok_or_else(|| ApiError::NotFound("skin not found".into()))?;

    let mut buf = Cursor::new(Vec::new());
    skin.data
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| ApiError::Internal(format!("failed to encode png: {e}")))?;
    Ok((
        [(header::CONTENT_TYPE, "image/png")],
        Body::from(buf.into_inner()),
    )
        .into_response())
}

#[utoipa::path(
    get,
    path = "/v3/player/face",
    description = "Renders a player's face as a PNG. Requires the `Player Data` permission or an Admin key.",
    params(
        ("player" = String, Query, description = "Player identifier: username, dashed UUID, or undashed UUID"),
        ("size" = Option<u32>, Query, description = "Face size in pixels (default 128, max 512)"),
    ),
    responses(
        (status = 200, description = "Rendered player face PNG", content_type = "image/png"),
        (status = 400, description = "Invalid identifier", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Skin not found", body = ErrorResponse),
        (status = 500, description = "Skin rendering unavailable", body = ErrorResponse),
    ),
    tag = "Internal",
    security(("api_key" = []))
)]
pub async fn player_face(
    State(state): State<AppState>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<FaceQuery>,
) -> Result<Response, ApiError> {
    if let Some(Extension(ref dev)) = dev_auth {
        dev.require(permissions::PLAYER_DATA)?;
    }
    let provider = state
        .skin_provider
        .as_ref()
        .ok_or_else(|| ApiError::Internal("skin rendering unavailable".into()))?;
    let identifier = query.identifier()?;
    let (uuid, _) = resolve_identifier(&state, identifier).await?;
    let size = query.size.unwrap_or(128).clamp(8, 512);
    let png = provider
        .fetch_face(&uuid, size)
        .await
        .ok_or_else(|| ApiError::NotFound("skin not found".into()))?;
    Ok(([(header::CONTENT_TYPE, "image/png")], Body::from(png)).into_response())
}

#[utoipa::path(
    get,
    path = "/v3/player/body",
    description = "Renders a player's full body as a PNG. Requires the `Player Data` permission or an Admin key.",
    params(
        ("player" = String, Query, description = "Player identifier: username, dashed UUID, or undashed UUID"),
        ("width" = Option<u32>, Query, description = "Output width (default 400)"),
        ("height" = Option<u32>, Query, description = "Output height (default 600)"),
    ),
    responses(
        (status = 200, description = "Rendered player body PNG", content_type = "image/png"),
        (status = 400, description = "Invalid identifier", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Skin not found", body = ErrorResponse),
        (status = 500, description = "Skin rendering unavailable", body = ErrorResponse),
    ),
    tag = "Internal",
    security(("api_key" = []))
)]
pub async fn player_body(
    State(state): State<AppState>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<BodyQuery>,
) -> Result<Response, ApiError> {
    if let Some(Extension(ref dev)) = dev_auth {
        dev.require(permissions::PLAYER_DATA)?;
    }
    let provider = state
        .skin_provider
        .as_ref()
        .ok_or_else(|| ApiError::Internal("skin rendering unavailable".into()))?;
    let identifier = query.identifier()?;
    let (uuid, _) = resolve_identifier(&state, identifier).await?;
    let w = query.width.unwrap_or(400).clamp(32, 2048);
    let h = query.height.unwrap_or(600).clamp(32, 2048);
    let image = provider
        .fetch(&uuid, w, h)
        .await
        .ok_or_else(|| ApiError::NotFound("skin not found".into()))?
        .data;

    let mut buf = Cursor::new(Vec::new());
    image
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| ApiError::Internal(format!("failed to encode png: {e}")))?;
    Ok((
        [(header::CONTENT_TYPE, "image/png")],
        Body::from(buf.into_inner()),
    )
        .into_response())
}
