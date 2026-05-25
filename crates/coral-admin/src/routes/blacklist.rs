use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
    tag_type: Option<String>,
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

#[derive(Serialize, FromRow, Clone)]
struct PlayerRow {
    id: i64,
    uuid: String,
    is_locked: bool,
    lock_reason: Option<String>,
    locked_by: Option<i64>,
    locked_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, FromRow, Clone)]
struct Tag {
    id: i64,
    player_id: i64,
    tag_type: String,
    reason: String,
    added_by: i64,
    added_on: DateTime<Utc>,
    hide_username: bool,
    removed_by: Option<i64>,
    removed_on: Option<DateTime<Utc>>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let pool = state.db.pool();

    let (count_sql, list_sql, search_pattern) = match (&params.search, &params.tag_type) {
        (Some(search), Some(tag_type)) => {
            let pattern = format!("%{search}%");
            (
                r#"SELECT COUNT(DISTINCT bp.id) FROM blacklist_players bp
                   JOIN player_tags pt ON pt.player_id = bp.id
                   WHERE bp.uuid LIKE $1 AND pt.tag_type = $2 AND pt.removed_on IS NULL"#,
                r#"SELECT DISTINCT bp.id, bp.uuid, bp.is_locked, bp.lock_reason, bp.locked_by, bp.locked_at
                   FROM blacklist_players bp
                   JOIN player_tags pt ON pt.player_id = bp.id
                   WHERE bp.uuid LIKE $1 AND pt.tag_type = $2 AND pt.removed_on IS NULL
                   ORDER BY bp.id DESC LIMIT $3 OFFSET $4"#,
                Some((pattern, tag_type.clone())),
            )
        }
        (Some(search), None) => {
            let pattern = format!("%{search}%");
            (
                "SELECT COUNT(*) FROM blacklist_players WHERE uuid LIKE $1",
                r#"SELECT id, uuid, is_locked, lock_reason, locked_by, locked_at
                   FROM blacklist_players WHERE uuid LIKE $1
                   ORDER BY id DESC LIMIT $2 OFFSET $3"#,
                Some((pattern, String::new())),
            )
        }
        (None, Some(tag_type)) => (
            r#"SELECT COUNT(DISTINCT bp.id) FROM blacklist_players bp
               JOIN player_tags pt ON pt.player_id = bp.id
               WHERE pt.tag_type = $1 AND pt.removed_on IS NULL"#,
            r#"SELECT DISTINCT bp.id, bp.uuid, bp.is_locked, bp.lock_reason, bp.locked_by, bp.locked_at
               FROM blacklist_players bp
               JOIN player_tags pt ON pt.player_id = bp.id
               WHERE pt.tag_type = $1 AND pt.removed_on IS NULL
               ORDER BY bp.id DESC LIMIT $2 OFFSET $3"#,
            Some((String::new(), tag_type.clone())),
        ),
        (None, None) => (
            "SELECT COUNT(*) FROM blacklist_players",
            r#"SELECT id, uuid, is_locked, lock_reason, locked_by, locked_at
               FROM blacklist_players ORDER BY id DESC LIMIT $1 OFFSET $2"#,
            None,
        ),
    };

    let (total, players): (i64, Vec<PlayerRow>) = match search_pattern {
        Some((ref pattern, ref tag_type)) if !pattern.is_empty() && !tag_type.is_empty() => {
            let total = sqlx::query_scalar(count_sql)
                .bind(pattern)
                .bind(tag_type)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let players = sqlx::query_as(list_sql)
                .bind(pattern)
                .bind(tag_type)
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
            (total, players)
        }
        Some((ref pattern, _)) if !pattern.is_empty() => {
            let total = sqlx::query_scalar(count_sql)
                .bind(pattern)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let players = sqlx::query_as(list_sql)
                .bind(pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
            (total, players)
        }
        Some((_, ref tag_type)) if !tag_type.is_empty() => {
            let total = sqlx::query_scalar(count_sql)
                .bind(tag_type)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let players = sqlx::query_as(list_sql)
                .bind(tag_type)
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
            (total, players)
        }
        _ => {
            let total = sqlx::query_scalar(count_sql)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            let players = sqlx::query_as(list_sql)
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
            (total, players)
        }
    };

    let player_ids: Vec<i64> = players.iter().map(|p| p.id).collect();
    let all_tags: Vec<Tag> = if player_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as(
            r#"SELECT id, player_id, tag_type, reason, added_by, added_on, hide_username, removed_by, removed_on
               FROM player_tags WHERE player_id = ANY($1) AND removed_on IS NULL
               ORDER BY added_on DESC"#,
        )
        .bind(&player_ids)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    };

    let players = players
        .into_iter()
        .map(|p| {
            let tags = all_tags
                .iter()
                .filter(|t| t.player_id == p.id)
                .cloned()
                .collect();
            PlayerWithTags {
                id: p.id,
                uuid: p.uuid,
                is_locked: p.is_locked,
                lock_reason: p.lock_reason,
                locked_by: p.locked_by,
                locked_at: p.locked_at,
                tags,
            }
        })
        .collect();

    Json(ListResponse { total, players })
}

#[derive(Serialize)]
struct DetailResponse {
    player: PlayerRow,
    tags: Vec<Tag>,
    tag_history: Vec<Tag>,
}

async fn detail(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Json<Option<DetailResponse>> {
    let pool = state.db.pool();

    let player = sqlx::query_as::<_, PlayerRow>(
        "SELECT id, uuid, is_locked, lock_reason, locked_by, locked_at FROM blacklist_players WHERE uuid = $1",
    )
    .bind(&uuid)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let Some(player) = player else {
        return Json(None);
    };

    let tags = sqlx::query_as::<_, Tag>(
        r#"SELECT id, player_id, tag_type, reason, added_by, added_on, hide_username, removed_by, removed_on
           FROM player_tags WHERE player_id = $1 AND removed_on IS NULL
           ORDER BY added_on DESC"#,
    )
    .bind(player.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let tag_history = sqlx::query_as::<_, Tag>(
        r#"SELECT id, player_id, tag_type, reason, added_by, added_on, hide_username, removed_by, removed_on
           FROM player_tags WHERE player_id = $1 AND removed_on IS NOT NULL
           ORDER BY removed_on DESC"#,
    )
    .bind(player.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Json(Some(DetailResponse {
        player,
        tags,
        tag_history,
    }))
}
