use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use database::GuildCacheRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{guild_id}", get(detail))
        .route("/{guild_id}/at", get(at))
}

#[derive(Deserialize)]
struct ListParams {
    search: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize, FromRow)]
struct GuildRow {
    guild_id: String,
    name: String,
    tag: Option<String>,
    level: i32,
    member_count: i32,
    experience: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    guilds: Vec<GuildRow>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let pool = state.db.pool();
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let order = match params.sort.as_deref() {
        Some("members") => "member_count DESC",
        Some("level") => "level DESC",
        Some("experience") => "experience DESC",
        _ => "updated_at DESC",
    };
    let cols = "guild_id, name, tag, level, member_count, experience, updated_at";

    let (total, guilds) = match params
        .search
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(q) => {
            let pattern = format!("%{}%", q.to_lowercase());
            let total = sqlx::query_scalar(
                "SELECT COUNT(*) FROM guild_current
                 WHERE lower(name) LIKE $1 OR lower(tag) LIKE $1 OR guild_id = $2",
            )
            .bind(&pattern)
            .bind(&q)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            let guilds = sqlx::query_as::<_, GuildRow>(&format!(
                "SELECT {cols} FROM guild_current
                 WHERE lower(name) LIKE $1 OR lower(tag) LIKE $1 OR guild_id = $2
                 ORDER BY {order} LIMIT $3 OFFSET $4"
            ))
            .bind(&pattern)
            .bind(&q)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            (total, guilds)
        }
        None => {
            let total = sqlx::query_scalar("SELECT COUNT(*) FROM guild_current")
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let guilds = sqlx::query_as::<_, GuildRow>(&format!(
                "SELECT {cols} FROM guild_current ORDER BY {order} LIMIT $1 OFFSET $2"
            ))
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            (total, guilds)
        }
    };

    Json(ListResponse { total, guilds })
}

#[derive(Serialize)]
struct GuildView {
    guild_id: String,
    name: Option<String>,
    current: Option<Value>,
    timestamps: Vec<DateTime<Utc>>,
}

async fn detail(State(state): State<AppState>, Path(guild_id): Path<String>) -> Json<GuildView> {
    let cache = GuildCacheRepository::new(state.db.pool());
    let mut timestamps = cache
        .list_snapshot_timestamps(&guild_id, None, None)
        .await
        .unwrap_or_default();
    timestamps.reverse();
    let current = cache.get_current(&guild_id).await.ok().flatten();
    let name = current
        .as_ref()
        .and_then(|c| c.get("name"))
        .and_then(|n| n.as_str())
        .map(String::from);

    Json(GuildView {
        guild_id,
        name,
        current,
        timestamps,
    })
}

#[derive(Deserialize)]
struct AtParams {
    ts: String,
}

async fn at(
    State(state): State<AppState>,
    Path(guild_id): Path<String>,
    Query(params): Query<AtParams>,
) -> Json<Option<Value>> {
    let Some(ts) = parse_ts(&params.ts) else {
        return Json(None);
    };
    Json(
        GuildCacheRepository::new(state.db.pool())
            .get_at(&guild_id, ts)
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
