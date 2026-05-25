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
    limit: Option<i64>,
    offset: Option<i64>,
    search: Option<String>,
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    members: Vec<Summary>,
}

#[derive(Serialize, FromRow)]
struct Summary {
    id: i64,
    discord_id: i64,
    uuid: Option<String>,
    join_date: DateTime<Utc>,
    request_count: i64,
    is_admin: bool,
    is_mod: bool,
    is_private: bool,
    is_beta: bool,
    key_locked: bool,
    has_api_key: bool,
}

#[derive(Serialize)]
struct Detail {
    #[serde(flatten)]
    member: MemberRow,
    ips: Vec<IpRecord>,
    alt_accounts: Vec<AltAccount>,
}

#[derive(Serialize, FromRow)]
struct MemberRow {
    id: i64,
    discord_id: i64,
    uuid: Option<String>,
    api_key_preview: Option<String>,
    join_date: DateTime<Utc>,
    request_count: i64,
    is_admin: bool,
    is_mod: bool,
    is_private: bool,
    is_beta: bool,
    key_locked: bool,
    config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, FromRow)]
struct IpRecord {
    ip_address: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

#[derive(Serialize, FromRow)]
struct AltAccount {
    uuid: String,
    added_at: DateTime<Utc>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let pool = state.db.pool();

    let (total, members) = match params.search {
        Some(ref search) => {
            let pattern = format!("%{search}%");
            let total = sqlx::query_scalar(
                "SELECT COUNT(*) FROM members WHERE discord_id::text LIKE $1 OR uuid LIKE $1",
            )
            .bind(&pattern)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            let members = sqlx::query_as::<_, Summary>(
                r#"SELECT id, discord_id, uuid, join_date, request_count,
                          is_admin, is_mod, is_private, is_beta, key_locked,
                          api_key IS NOT NULL as has_api_key
                   FROM members
                   WHERE discord_id::text LIKE $1 OR uuid LIKE $1
                   ORDER BY id DESC LIMIT $2 OFFSET $3"#,
            )
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            (total, members)
        }
        None => {
            let total = sqlx::query_scalar("SELECT COUNT(*) FROM members")
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let members = sqlx::query_as::<_, Summary>(
                r#"SELECT id, discord_id, uuid, join_date, request_count,
                          is_admin, is_mod, is_private, is_beta, key_locked,
                          api_key IS NOT NULL as has_api_key
                   FROM members
                   ORDER BY id DESC LIMIT $1 OFFSET $2"#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            (total, members)
        }
    };

    Json(ListResponse { total, members })
}

async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Json<Option<Detail>> {
    let pool = state.db.pool();

    let member = sqlx::query_as::<_, MemberRow>(
        r#"SELECT id, discord_id, uuid, LEFT(api_key, 8) as api_key_preview,
                  join_date, request_count, is_admin, is_mod, is_private, is_beta,
                  key_locked, config, created_at, updated_at
           FROM members WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let Some(member) = member else {
        return Json(None);
    };

    let ips = sqlx::query_as::<_, IpRecord>(
        r#"SELECT ip_address::text, first_seen, last_seen
           FROM api_key_ips WHERE member_id = $1
           ORDER BY last_seen DESC"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let alt_accounts = sqlx::query_as::<_, AltAccount>(
        "SELECT uuid, added_at FROM minecraft_accounts WHERE member_id = $1 ORDER BY added_at DESC",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Json(Some(Detail {
        member,
        ips,
        alt_accounts,
    }))
}
