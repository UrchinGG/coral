use std::collections::HashMap;

use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use database::CacheRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::state::AppState;

const RECENT_WINDOW: i64 = 200_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{uuid}", get(detail))
        .route("/{uuid}/at", get(at))
}

#[derive(Deserialize)]
struct ListParams {
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct PlayerRow {
    uuid: String,
    username: Option<String>,
    last_snapshot_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct ListResponse {
    players: Vec<PlayerRow>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let pool = state.db.pool();

    if let Some(q) = params
        .search
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return Json(ListResponse {
            players: search(pool, &q).await,
        });
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    let rows: Vec<(String, Option<String>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT uuid, username, timestamp FROM (
            SELECT DISTINCT ON (uuid) uuid, username, timestamp FROM (
                SELECT uuid, username, timestamp FROM player_snapshots ORDER BY timestamp DESC LIMIT $1
            ) recent ORDER BY uuid, timestamp DESC
         ) latest ORDER BY timestamp DESC LIMIT $2 OFFSET $3",
    )
    .bind(RECENT_WINDOW)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let players = rows
        .into_iter()
        .map(|(uuid, username, ts)| PlayerRow {
            uuid,
            username,
            last_snapshot_at: Some(ts),
        })
        .collect();

    Json(ListResponse { players })
}

fn prefix_upper(prefix: &str) -> Option<String> {
    let mut chars: Vec<char> = prefix.chars().collect();
    while let Some(last) = chars.pop() {
        if let Some(next) = char::from_u32(last as u32 + 1) {
            chars.push(next);
            return Some(chars.into_iter().collect());
        }
    }
    None
}

async fn search(pool: &PgPool, q: &str) -> Vec<PlayerRow> {
    let cache = CacheRepository::new(pool);
    let normalized = q.replace('-', "").to_lowercase();

    let found: Vec<(String, Option<String>)> =
        if normalized.len() == 32 && normalized.chars().all(|c| c.is_ascii_hexdigit()) {
            vec![(
                normalized.clone(),
                cache.get_username(&normalized).await.ok().flatten(),
            )]
        } else {
            let lower = q.to_lowercase();
            let mut rows: Vec<(String, Option<String>)> = match prefix_upper(&lower) {
                Some(upper) => sqlx::query_as(
                    "SELECT DISTINCT ON (lower(username)) uuid, username FROM player_snapshots
                     WHERE lower(username) >= $1 AND lower(username) < $2
                     ORDER BY lower(username), timestamp DESC LIMIT 25",
                )
                .bind(&lower)
                .bind(upper),
                None => sqlx::query_as(
                    "SELECT DISTINCT ON (lower(username)) uuid, username FROM player_snapshots
                     WHERE lower(username) >= $1
                     ORDER BY lower(username), timestamp DESC LIMIT 25",
                )
                .bind(&lower),
            }
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            if let Ok(discord) = cache.find_by_discord_username(q).await {
                for (uuid, username) in discord {
                    if !rows.iter().any(|(u, _)| *u == uuid) {
                        rows.push((uuid, Some(username)));
                    }
                }
            }
            rows
        };

    let uuids: Vec<String> = found.iter().map(|(u, _)| u.clone()).collect();
    let stamps: HashMap<String, DateTime<Utc>> = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT uuid, MAX(timestamp) FROM player_snapshots WHERE uuid = ANY($1) GROUP BY uuid",
    )
    .bind(&uuids)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    found
        .into_iter()
        .map(|(uuid, username)| PlayerRow {
            last_snapshot_at: stamps.get(&uuid).cloned(),
            username,
            uuid,
        })
        .collect()
}

#[derive(Serialize)]
struct PlayerView {
    uuid: String,
    username: Option<String>,
    latest: Option<Value>,
    timestamps: Vec<DateTime<Utc>>,
}

async fn detail(State(state): State<AppState>, Path(uuid): Path<String>) -> Json<PlayerView> {
    let cache = CacheRepository::new(state.db.pool());
    let uuid = uuid.to_lowercase();
    let mut timestamps = cache
        .list_snapshot_timestamps(&uuid, None, None)
        .await
        .unwrap_or_default();
    timestamps.reverse();

    Json(PlayerView {
        username: cache.get_username(&uuid).await.ok().flatten(),
        latest: cache.get_latest_snapshot(&uuid).await.ok().flatten(),
        timestamps,
        uuid,
    })
}

#[derive(Deserialize)]
struct AtParams {
    ts: String,
}

async fn at(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    Query(params): Query<AtParams>,
) -> Json<Option<Value>> {
    let Some(ts) = parse_ts(&params.ts) else {
        return Json(None);
    };
    let cache = CacheRepository::new(state.db.pool());
    Json(
        cache
            .get_snapshot_at(&uuid.to_lowercase(), ts)
            .await
            .ok()
            .flatten(),
    )
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    s.parse::<i64>()
        .ok()
        .and_then(DateTime::from_timestamp_millis)
}
