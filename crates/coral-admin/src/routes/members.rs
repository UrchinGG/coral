use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};

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
    sort: Option<String>,
    dir: Option<String>,
    rank: Option<i16>,
    locked: Option<bool>,
    haskey: Option<bool>,
}

fn apply_filters(qb: &mut QueryBuilder<'_, Postgres>, p: &ListParams) {
    let mut sep = " WHERE ";
    if let Some(s) = p.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = format!("%{s}%");
        qb.push(sep)
            .push("(discord_id::text LIKE ")
            .push_bind(pattern.clone())
            .push(" OR uuid LIKE ")
            .push_bind(pattern)
            .push(")");
        sep = " AND ";
    }
    if let Some(rank) = p.rank {
        qb.push(sep).push("access_level >= ").push_bind(rank);
        sep = " AND ";
    }
    if p.locked.unwrap_or(false) {
        qb.push(sep).push("key_locked = true");
        sep = " AND ";
    }
    if p.haskey.unwrap_or(false) {
        qb.push(sep).push("api_key IS NOT NULL");
        sep = " AND ";
    }
    let _ = sep;
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
    access_level: i16,
    key_locked: bool,
    has_api_key: bool,
    #[sqlx(default)]
    is_owner: bool,
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
    access_level: i16,
    key_locked: bool,
    config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[sqlx(default)]
    is_owner: bool,
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
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let pool = state.db.pool();

    let order = match params.sort.as_deref() {
        Some("requests") => "request_count",
        Some("joined") => "join_date",
        Some("access") => "access_level",
        _ => "id",
    };
    let dir = if params.dir.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let mut count = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM members");
    apply_filters(&mut count, &params);
    let total = count
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let mut q = QueryBuilder::<Postgres>::new(
        "SELECT id, discord_id, uuid, join_date, request_count, access_level, key_locked,
                api_key IS NOT NULL as has_api_key FROM members",
    );
    apply_filters(&mut q, &params);
    q.push(format!(" ORDER BY {order} {dir} LIMIT "))
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let mut members = q
        .build_query_as::<Summary>()
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    for m in &mut members {
        m.is_owner = state.owner_ids.contains(&m.discord_id);
    }

    Json(ListResponse { total, members })
}

async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Json<Option<Detail>> {
    let pool = state.db.pool();

    let member = sqlx::query_as::<_, MemberRow>(
        r#"SELECT id, discord_id, uuid, LEFT(api_key, 8) as api_key_preview,
                  join_date, request_count, access_level,
                  key_locked, config, created_at, updated_at
           FROM members WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let Some(mut member) = member else {
        return Json(None);
    };
    member.is_owner = state.owner_ids.contains(&member.discord_id);

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
