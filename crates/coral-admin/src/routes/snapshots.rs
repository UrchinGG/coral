use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{id}", get(detail))
}

#[derive(Deserialize)]
struct ListParams {
    uuid: Option<String>,
    username: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    snapshots: Vec<SnapshotSummary>,
}

#[derive(Serialize, FromRow)]
struct SnapshotSummary {
    id: i64,
    uuid: String,
    username: Option<String>,
    timestamp: DateTime<Utc>,
    source: Option<String>,
    is_baseline: bool,
}

#[derive(Serialize, FromRow)]
struct SnapshotDetail {
    id: i64,
    uuid: String,
    username: Option<String>,
    timestamp: DateTime<Utc>,
    discord_id: Option<i64>,
    source: Option<String>,
    is_baseline: bool,
    data: serde_json::Value,
    created_at: DateTime<Utc>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let pool = state.db.pool();

    let (total, snapshots) = match (&params.uuid, &params.username) {
        (Some(uuid), _) => {
            let total = sqlx::query_scalar("SELECT COUNT(*) FROM player_snapshots WHERE uuid = $1")
                .bind(uuid)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let snapshots = sqlx::query_as::<_, SnapshotSummary>(
                r#"SELECT id, uuid, username, timestamp, source, is_baseline
                   FROM player_snapshots WHERE uuid = $1
                   ORDER BY timestamp DESC LIMIT $2 OFFSET $3"#,
            )
            .bind(uuid)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            (total, snapshots)
        }
        (None, Some(username)) => {
            let pattern = format!("%{username}%");
            let total =
                sqlx::query_scalar("SELECT COUNT(*) FROM player_snapshots WHERE username ILIKE $1")
                    .bind(&pattern)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);
            let snapshots = sqlx::query_as::<_, SnapshotSummary>(
                r#"SELECT id, uuid, username, timestamp, source, is_baseline
                   FROM player_snapshots WHERE username ILIKE $1
                   ORDER BY timestamp DESC LIMIT $2 OFFSET $3"#,
            )
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            (total, snapshots)
        }
        (None, None) => {
            let total = sqlx::query_scalar("SELECT COUNT(*) FROM player_snapshots")
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let snapshots = sqlx::query_as::<_, SnapshotSummary>(
                r#"SELECT id, uuid, username, timestamp, source, is_baseline
                   FROM player_snapshots
                   ORDER BY timestamp DESC LIMIT $1 OFFSET $2"#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            (total, snapshots)
        }
    };

    Json(ListResponse { total, snapshots })
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<Option<SnapshotDetail>> {
    Json(
        sqlx::query_as::<_, SnapshotDetail>(
            r#"SELECT id, uuid, username, timestamp, discord_id, source, is_baseline, data, created_at
               FROM player_snapshots WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(state.db.pool())
        .await
        .ok()
        .flatten(),
    )
}
