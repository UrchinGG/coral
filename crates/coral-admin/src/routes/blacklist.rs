use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{uuid}", get(detail))
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<i64>,
    offset: Option<i64>,
    search: Option<String>,
    field: Option<String>,
    tag_type: Option<String>,
    dir: Option<String>,
}

fn bl_filters(qb: &mut QueryBuilder<'_, Postgres>, p: &ListParams) {
    qb.push(" WHERE at.kind = 'tag_set'");
    if let Some(s) = p.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match p.field.as_deref() {
            Some("tagger") => match s.parse::<i64>() {
                Ok(id) => {
                    qb.push(" AND at.author = ").push_bind(id);
                }
                Err(_) => {
                    qb.push(" AND false");
                }
            },
            Some("reason") => {
                qb.push(" AND at.reason ILIKE ").push_bind(format!("%{s}%"));
            }
            _ => {
                qb.push(" AND at.uuid LIKE ").push_bind(format!("%{s}%"));
            }
        }
    }
    if let Some(t) = p.tag_type.as_deref().filter(|t| !t.is_empty()) {
        qb.push(" AND at.tag_type = ").push_bind(t.to_string());
    }
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    players: Vec<PlayerWithTags>,
}

#[derive(Serialize)]
struct PlayerWithTags {
    id: i64,
    uuid: String,
    is_locked: bool,
    lock_reason: Option<String>,
    locked_by: Option<i64>,
    locked_at: Option<DateTime<Utc>>,
    tags: Vec<Tag>,
}

#[derive(FromRow, Clone)]
struct PlayerListRow {
    id: i64,
    uuid: String,
}

#[derive(Serialize, FromRow, Clone)]
struct LockState {
    uuid: String,
    is_locked: bool,
    lock_reason: Option<String>,
    locked_by: Option<i64>,
    locked_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, FromRow, Clone)]
struct Tag {
    id: i64,
    uuid: String,
    tag_type: String,
    reason: Option<String>,
    #[serde(rename = "added_by")]
    author: Option<i64>,
    #[serde(rename = "added_on")]
    ts: DateTime<Utc>,
    hide_username: Option<bool>,
}

#[derive(Serialize, FromRow, Clone)]
struct RemovedTag {
    add_id: i64,
    uuid: String,
    tag_type: String,
    reason: Option<String>,
    added_by: Option<i64>,
    added_on: DateTime<Utc>,
    removed_by: Option<i64>,
    removed_on: DateTime<Utc>,
}

const ACTIVE_TAGS_CTE: &str = "active_tags AS (
    SELECT DISTINCT ON (uuid, tag_type) id, uuid, tag_type, reason, author, ts, hide_username, kind
    FROM player_events
    WHERE kind IN ('tag_set', 'tag_clear')
    ORDER BY uuid, tag_type, ts DESC, id DESC
)";

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let pool = state.db.pool();

    let (total, players) = fetch_players(pool, &params, limit, offset).await;

    let uuids: Vec<String> = players.iter().map(|p| p.uuid.clone()).collect();
    let (all_tags, lock_states) = if uuids.is_empty() {
        (vec![], vec![])
    } else {
        tokio::join!(
            fetch_tags_for(pool, &uuids),
            fetch_lock_states(pool, &uuids)
        )
    };

    let players = players
        .into_iter()
        .map(|p| {
            let lock = lock_states
                .iter()
                .find(|l| l.uuid == p.uuid)
                .cloned()
                .unwrap_or(LockState {
                    uuid: p.uuid.clone(),
                    is_locked: false,
                    lock_reason: None,
                    locked_by: None,
                    locked_at: None,
                });
            PlayerWithTags {
                id: p.id,
                uuid: p.uuid.clone(),
                is_locked: lock.is_locked,
                lock_reason: lock.lock_reason,
                locked_by: lock.locked_by,
                locked_at: lock.locked_at,
                tags: all_tags
                    .iter()
                    .filter(|t| t.uuid == p.uuid)
                    .cloned()
                    .collect(),
            }
        })
        .collect();

    Json(ListResponse { total, players })
}

async fn fetch_players(
    pool: &PgPool,
    params: &ListParams,
    limit: i64,
    offset: i64,
) -> (i64, Vec<PlayerListRow>) {
    let dir = if params.dir.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let mut count = QueryBuilder::<Postgres>::new(format!(
        "WITH {ACTIVE_TAGS_CTE} SELECT COUNT(DISTINCT at.uuid) FROM active_tags at"
    ));
    bl_filters(&mut count, params);
    let total = count
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let mut q = QueryBuilder::<Postgres>::new(format!(
        "WITH {ACTIVE_TAGS_CTE} SELECT MAX(at.id) AS id, at.uuid FROM active_tags at"
    ));
    bl_filters(&mut q, params);
    q.push(format!(
        " GROUP BY at.uuid ORDER BY MAX(at.ts) {dir} LIMIT "
    ))
    .push_bind(limit)
    .push(" OFFSET ")
    .push_bind(offset);
    let players = q
        .build_query_as::<PlayerListRow>()
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    (total, players)
}

async fn fetch_tags_for(pool: &PgPool, uuids: &[String]) -> Vec<Tag> {
    sqlx::query_as(&format!(
        "WITH {ACTIVE_TAGS_CTE}
         SELECT id, uuid, tag_type, reason, author, ts, hide_username
         FROM active_tags
         WHERE uuid = ANY($1) AND kind = 'tag_set'
         ORDER BY ts DESC"
    ))
    .bind(uuids)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

async fn fetch_lock_states(pool: &PgPool, uuids: &[String]) -> Vec<LockState> {
    sqlx::query_as(
        "SELECT DISTINCT ON (uuid)
             uuid,
             (kind = 'lock') AS is_locked,
             reason AS lock_reason,
             author AS locked_by,
             ts AS locked_at
         FROM player_events
         WHERE uuid = ANY($1) AND kind IN ('lock', 'unlock')
         ORDER BY uuid, ts DESC, id DESC",
    )
    .bind(uuids)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

#[derive(Serialize)]
struct DetailResponse {
    player: DetailPlayer,
    tags: Vec<Tag>,
    tag_history: Vec<RemovedTag>,
}

#[derive(Serialize)]
struct DetailPlayer {
    id: i64,
    uuid: String,
    is_locked: bool,
    lock_reason: Option<String>,
    locked_by: Option<i64>,
    locked_at: Option<DateTime<Utc>>,
}

async fn detail(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Json<Option<DetailResponse>> {
    let pool = state.db.pool();

    let Some(max_id): Option<i64> =
        sqlx::query_scalar("SELECT MAX(id) FROM player_events WHERE uuid = $1")
            .bind(&uuid)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten()
    else {
        return Json(None);
    };

    let lock = fetch_lock_states(pool, std::slice::from_ref(&uuid))
        .await
        .into_iter()
        .next();

    let tags = sqlx::query_as::<_, Tag>(&format!(
        "WITH {ACTIVE_TAGS_CTE}
         SELECT id, uuid, tag_type, reason, author, ts, hide_username
         FROM active_tags WHERE uuid = $1 AND kind = 'tag_set'
         ORDER BY ts DESC"
    ))
    .bind(&uuid)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let tag_history: Vec<RemovedTag> = sqlx::query_as(
        "WITH events AS (
             SELECT id, tag_type, reason, author, ts, kind,
                    LEAD(author) OVER w AS next_author,
                    LEAD(ts) OVER w AS next_ts,
                    LEAD(kind) OVER w AS next_kind
             FROM player_events
             WHERE uuid = $1 AND kind IN ('tag_set', 'tag_clear')
             WINDOW w AS (PARTITION BY tag_type ORDER BY ts, id)
         )
         SELECT id AS add_id, $1::text AS uuid, tag_type, reason,
                author AS added_by, ts AS added_on,
                next_author AS removed_by, next_ts AS removed_on
         FROM events
         WHERE kind = 'tag_set' AND next_kind = 'tag_clear' AND next_ts IS NOT NULL
         ORDER BY next_ts DESC",
    )
    .bind(&uuid)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Json(Some(DetailResponse {
        player: DetailPlayer {
            id: max_id,
            uuid: uuid.clone(),
            is_locked: lock.as_ref().map(|l| l.is_locked).unwrap_or(false),
            lock_reason: lock.as_ref().and_then(|l| l.lock_reason.clone()),
            locked_by: lock.as_ref().and_then(|l| l.locked_by),
            locked_at: lock.as_ref().and_then(|l| l.locked_at),
        },
        tags,
        tag_history,
    }))
}
