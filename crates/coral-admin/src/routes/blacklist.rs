use axum::http::StatusCode;
use axum::{Extension, Json, Router, extract::*, routing::get, routing::post};
use chrono::{DateTime, Utc};
use database::{AddOutcome, BlacklistRepository};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};

use crate::auth::AdminActor;
use crate::identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{uuid}", get(detail))
        .route("/{uuid}/tags", post(add_tag))
        .route("/{uuid}/tags/{tag_type}", axum::routing::delete(remove_tag))
        .route("/{uuid}/lock", post(lock))
        .route("/{uuid}/unlock", post(unlock))
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

fn bl_filters(qb: &mut QueryBuilder<'_, Postgres>, p: &ListParams, ign_uuid: Option<&str>) {
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
                qb.push(" AND (at.uuid LIKE ").push_bind(format!("%{s}%"));
                if let Some(uuid) = ign_uuid {
                    qb.push(" OR at.uuid = ").push_bind(uuid.to_string());
                }
                qb.push(")");
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
    minecraft_username: Option<String>,
    is_locked: bool,
    lock_reason: Option<String>,
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    locked_by: Option<i64>,
    locked_by_username: Option<String>,
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
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    locked_by: Option<i64>,
    locked_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, FromRow, Clone)]
struct Tag {
    id: i64,
    uuid: String,
    tag_type: String,
    reason: Option<String>,
    #[serde(
        rename = "added_by",
        serialize_with = "crate::serde_id::discord_id_opt"
    )]
    author: Option<i64>,
    #[sqlx(default)]
    added_by_username: Option<String>,
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
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    added_by: Option<i64>,
    added_on: DateTime<Utc>,
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
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

    let ign_uuid = match params.search.as_deref().map(str::trim) {
        Some(s) if params.field.is_none() && identity::looks_like_ign(s) => {
            identity::resolve_minecraft_uuid(&state, s).await
        }
        _ => None,
    };

    let (total, players) = fetch_players(pool, &params, ign_uuid.as_deref(), limit, offset).await;

    let uuids: Vec<String> = players.iter().map(|p| p.uuid.clone()).collect();
    let (all_tags, lock_states) = if uuids.is_empty() {
        (vec![], vec![])
    } else {
        tokio::join!(
            fetch_tags_for(pool, &uuids),
            fetch_lock_states(pool, &uuids)
        )
    };

    let discord_ids: Vec<i64> = all_tags
        .iter()
        .filter_map(|t| t.author)
        .chain(lock_states.iter().filter_map(|l| l.locked_by))
        .collect();
    let names = identity::resolve(&state, &discord_ids, &uuids).await;

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
                minecraft_username: names.minecraft.get(&p.uuid).cloned(),
                uuid: p.uuid.clone(),
                is_locked: lock.is_locked,
                lock_reason: lock.lock_reason,
                locked_by_username: lock.locked_by.and_then(|a| names.discord.get(&a).cloned()),
                locked_by: lock.locked_by,
                locked_at: lock.locked_at,
                tags: all_tags
                    .iter()
                    .filter(|t| t.uuid == p.uuid)
                    .cloned()
                    .map(|mut t| {
                        t.added_by_username = t.author.and_then(|a| names.discord.get(&a).cloned());
                        t
                    })
                    .collect(),
            }
        })
        .collect();

    Json(ListResponse { total, players })
}

async fn fetch_players(
    pool: &PgPool,
    params: &ListParams,
    ign_uuid: Option<&str>,
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
    bl_filters(&mut count, params, ign_uuid);
    let total = count
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let mut q = QueryBuilder::<Postgres>::new(format!(
        "WITH {ACTIVE_TAGS_CTE} SELECT MAX(at.id) AS id, at.uuid FROM active_tags at"
    ));
    bl_filters(&mut q, params, ign_uuid);
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
             (kind = 'lock' AND (expires_at IS NULL OR expires_at > NOW())) AS is_locked,
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
    minecraft_username: Option<String>,
    is_locked: bool,
    lock_reason: Option<String>,
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    locked_by: Option<i64>,
    locked_by_username: Option<String>,
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

    let mut tags = sqlx::query_as::<_, Tag>(&format!(
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

    let discord_ids: Vec<i64> = tags
        .iter()
        .filter_map(|t| t.author)
        .chain(lock.as_ref().and_then(|l| l.locked_by))
        .chain(tag_history.iter().filter_map(|t| t.added_by))
        .chain(tag_history.iter().filter_map(|t| t.removed_by))
        .collect();
    let names = identity::resolve(&state, &discord_ids, std::slice::from_ref(&uuid)).await;
    for t in &mut tags {
        t.added_by_username = t.author.and_then(|a| names.discord.get(&a).cloned());
    }

    Json(Some(DetailResponse {
        player: DetailPlayer {
            id: max_id,
            minecraft_username: names.minecraft.get(&uuid).cloned(),
            uuid: uuid.clone(),
            is_locked: lock.as_ref().map(|l| l.is_locked).unwrap_or(false),
            lock_reason: lock.as_ref().and_then(|l| l.lock_reason.clone()),
            locked_by_username: lock
                .as_ref()
                .and_then(|l| l.locked_by)
                .and_then(|a| names.discord.get(&a).cloned()),
            locked_by: lock.as_ref().and_then(|l| l.locked_by),
            locked_at: lock.as_ref().and_then(|l| l.locked_at),
        },
        tags,
        tag_history,
    }))
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
}

#[derive(Deserialize)]
struct AddTagRequest {
    tag_type: String,
    reason: String,
    #[serde(default)]
    hide_username: bool,
}

async fn add_tag(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(uuid): Path<String>,
    Json(req): Json<AddTagRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let repo = BlacklistRepository::new(state.db.pool());
    let outcome = repo
        .add_event(
            &uuid,
            &req.tag_type,
            &req.reason,
            req.hide_username,
            None,
            None,
            Some(actor.discord_id),
            &[],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match outcome {
        AddOutcome::Inserted(_) => {
            audit(
                &state,
                actor,
                "add_tag",
                &uuid,
                json!({"tag_type": req.tag_type, "reason": req.reason}),
            )
            .await;
            Ok(Json(OkResponse { ok: true }))
        }
        AddOutcome::Conflict(_) => Err(StatusCode::CONFLICT),
    }
}

async fn remove_tag(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((uuid, tag_type)): Path<(String, String)>,
) -> Result<Json<OkResponse>, StatusCode> {
    let removed = BlacklistRepository::new(state.db.pool())
        .remove_event(&uuid, &tag_type, Some(actor.discord_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !removed {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(
        &state,
        actor,
        "remove_tag",
        &uuid,
        json!({"tag_type": tag_type}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct LockRequest {
    reason: Option<String>,
}

async fn lock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(uuid): Path<String>,
    Json(req): Json<LockRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    BlacklistRepository::new(state.db.pool())
        .lock_event(&uuid, req.reason.as_deref(), actor.discord_id, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "lock_player",
        &uuid,
        json!({"reason": req.reason}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn unlock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(uuid): Path<String>,
) -> Result<Json<OkResponse>, StatusCode> {
    let unlocked = BlacklistRepository::new(state.db.pool())
        .unlock_event(&uuid, actor.discord_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !unlocked {
        return Err(StatusCode::CONFLICT);
    }
    audit(&state, actor, "unlock_player", &uuid, json!({})).await;
    Ok(Json(OkResponse { ok: true }))
}

async fn audit(
    state: &AppState,
    actor: AdminActor,
    action: &str,
    uuid: &str,
    details: serde_json::Value,
) {
    crate::audit::log(state, actor.discord_id, action, uuid, details).await;
}
