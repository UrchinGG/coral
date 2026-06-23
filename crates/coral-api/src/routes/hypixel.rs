use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

use clients::{ClientError, is_uuid, normalize_uuid};
use database::{CacheRepository, GuildCurrentRepository, permissions};

use crate::{
    auth::DeveloperKeyAuth,
    cache::{SNAPSHOT_SOURCE, parse_cache_age},
    error::ApiError,
    state::AppState,
};

use super::player::{pick_identifier, resolve_identifier, resolve_player_data};

#[derive(Deserialize)]
pub(crate) struct PlayerQuery {
    pub player: Option<String>,
    pub uuid: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub max_cache_age: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GuildQuery {
    pub id: Option<String>,
    pub player: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub max_cache_age: Option<String>,
}

async fn serve_guild(
    max_cache_age: Option<Duration>,
    current: Option<(Value, DateTime<Utc>)>,
    fetch: impl std::future::Future<Output = Result<Option<Value>, ClientError>>,
) -> Result<(Option<Value>, bool), ApiError> {
    if let (Some(max_age), Some((raw, updated))) = (max_cache_age, &current) {
        if Utc::now() - *updated <= max_age {
            return Ok((Some(raw.clone()), true));
        }
    }

    match fetch.await {
        Ok(data) => Ok((data, false)),
        Err(err) if max_cache_age.is_some() => match current {
            Some((raw, _)) => Ok((Some(raw), true)),
            None => Err(err.into()),
        },
        Err(err) => Err(err.into()),
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/hypixel/player", get(player))
        .route("/hypixel/guild", get(guild))
}

#[utoipa::path(
    get,
    path = "/v3/hypixel/player",
    description = "Returns Hypixel's player payload unchanged, wrapped in a `player` field. Look up a player by `player`. Set `max_cache_age` (for example `5m`, `1h`, or a number of seconds) to serve a stored snapshot within that age instead of calling Hypixel, and to fall back to the latest snapshot when Hypixel is unreachable; such responses set `stale` to true. Use `/v3/player/profile` instead unless you specifically require the unmodified response. Requires the `Hypixel` permission or an Admin key.",
    params(
        ("player" = String, Query, description = "Player identifier: username, dashed UUID, or undashed UUID"),
        ("max_cache_age" = Option<String>, Query, description = "Accept a stored snapshot up to this age (e.g. `5m`, `1h`, `30s`)"),
    ),
    responses(
        (status = 200, description = "Raw Hypixel player payload", body = serde_json::Value),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Player not found", body = crate::error::ErrorResponse),
        (status = 502, description = "External API error", body = crate::error::ErrorResponse),
    ),
    tag = "Hypixel",
    security(("api_key" = []))
)]
pub async fn player(
    State(state): State<AppState>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<PlayerQuery>,
) -> Result<Json<Value>, ApiError> {
    if let Some(Extension(ref dev)) = dev_auth {
        dev.require(permissions::HYPIXEL)?;
    }
    let max_cache_age = query
        .max_cache_age
        .as_deref()
        .map(parse_cache_age)
        .transpose()?;
    let identifier = pick_identifier(
        query.player.as_deref(),
        query.uuid.as_deref(),
        query.name.as_deref(),
    )?;
    let (uuid, username_hint) = resolve_identifier(&state, identifier).await?;
    let (data, stale) = resolve_player_data(&state, &uuid, max_cache_age).await?;

    if !stale {
        if let Some(ref player) = data {
            let username = username_hint
                .or_else(|| player["displayname"].as_str().map(String::from))
                .unwrap_or_else(|| uuid.clone());
            let pool = state.db.pool().clone();
            let uuid = uuid.clone();
            let player = player.clone();
            tokio::spawn(async move {
                let _ = CacheRepository::new(&pool)
                    .store_snapshot(&uuid, &player, None, Some(SNAPSHOT_SOURCE), Some(&username))
                    .await;
            });
        }
    }

    Ok(Json(
        json!({ "success": true, "stale": stale, "player": data }),
    ))
}

#[utoipa::path(
    get,
    path = "/v3/hypixel/guild",
    description = "Returns Hypixel's guild payload unchanged, wrapped in a `guild` field. Look up a guild by `id`, by `player` (a member's UUID or username), or by `name` (the guild's name). Set `max_cache_age` (for example `5m`, `1h`, or a number of seconds) to serve the latest stored guild within that age instead of calling Hypixel, and to fall back to it when Hypixel is unreachable; such responses set `stale` to true. Requires the `Hypixel` permission or an Admin key.",
    params(
        ("id" = Option<String>, Query, description = "Guild id"),
        ("player" = Option<String>, Query, description = "A member's UUID or username"),
        ("name" = Option<String>, Query, description = "Guild name"),
        ("max_cache_age" = Option<String>, Query, description = "Accept the latest stored guild up to this age (e.g. `5m`, `1h`, `30s`)"),
    ),
    responses(
        (status = 200, description = "Raw Hypixel guild payload", body = serde_json::Value),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 502, description = "External API error", body = crate::error::ErrorResponse),
    ),
    tag = "Hypixel",
    security(("api_key" = []))
)]
pub async fn guild(
    State(state): State<AppState>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<GuildQuery>,
) -> Result<Json<Value>, ApiError> {
    if let Some(Extension(ref dev)) = dev_auth {
        dev.require(permissions::HYPIXEL)?;
    }
    let max_cache_age = query
        .max_cache_age
        .as_deref()
        .map(parse_cache_age)
        .transpose()?;
    let hypixel = state.require_hypixel()?;
    let current = GuildCurrentRepository::new(state.db.pool());

    let (data, stale) = match (
        query.id.as_deref(),
        query.player.as_deref(),
        query.name.as_deref(),
    ) {
        (Some(id), _, _) => {
            serve_guild(
                max_cache_age,
                current.get(id).await?,
                hypixel.get_guild_by_id(id),
            )
            .await?
        }
        (_, Some(player), _) => {
            let uuid = if is_uuid(player) {
                normalize_uuid(player)
            } else {
                normalize_uuid(&state.mojang.resolve(player).await?.uuid)
            };
            serve_guild(
                max_cache_age,
                current.get_by_member(&uuid).await?,
                hypixel.get_guild_by_player(&uuid),
            )
            .await?
        }
        (_, _, Some(name)) => {
            serve_guild(
                max_cache_age,
                current.get_by_name(name).await?,
                hypixel.get_guild_by_name(name),
            )
            .await?
        }
        _ => {
            return Err(ApiError::BadRequest(
                "query parameter 'id', 'player', or 'name' required".into(),
            ));
        }
    };

    if !stale {
        if let Some(raw) = data.clone() {
            let state = state.clone();
            tokio::spawn(async move {
                crate::cache::cache_guild(&state, &raw).await;
            });
        }
    }

    Ok(Json(
        json!({ "success": true, "stale": stale, "guild": data }),
    ))
}
