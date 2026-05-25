use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list))
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    rate_limits: Vec<RateLimit>,
}

#[derive(Serialize, FromRow)]
struct RateLimit {
    id: i64,
    api_key: String,
    request_count: i64,
    created_at: DateTime<Utc>,
}

async fn list(State(state): State<AppState>) -> Json<ListResponse> {
    let pool = state.db.pool();

    let total = sqlx::query_scalar("SELECT COUNT(*) FROM rate_limits")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let rate_limits = sqlx::query_as::<_, RateLimit>(
        r#"SELECT id, LEFT(api_key, 8) as api_key, array_length(requests, 1) as request_count, created_at
           FROM rate_limits
           ORDER BY array_length(requests, 1) DESC NULLS LAST
           LIMIT 100"#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Json(ListResponse { total, rate_limits })
}
