use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};

use super::exact_json::exact_integers;
use super::profile::{account_ref, player_ref};
use super::{AccountTarget, CursorPage, parse_discord_id};
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::routes::ref_labels::label_request;
use crate::serde_ts;
use crate::state::AppState;

const TIMELINE_PAGE: usize = 50;
const MAX_TIMELINE_PAGE: usize = 200;
const CURSOR_SEPARATOR: char = '_';

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct TimelineEvent {
    key: String,
    #[serde(serialize_with = "serde_ts::micros")]
    at: DateTime<Utc>,
    kind: String,
    actor: Option<EntityRef>,
    target: Option<EntityRef>,
    target_text: Option<String>,
    player: Option<EntityRef>,
    action: Option<String>,
    detail: Option<String>,
    data: Value,
}

#[derive(FromRow)]
struct EventRow {
    key: String,
    at: DateTime<Utc>,
    kind: String,
    actor: Option<i64>,
    target: Option<String>,
    player: Option<String>,
    action: Option<String>,
    detail: Option<String>,
    data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimelineCursor {
    at: DateTime<Utc>,
    key: String,
}

const TIMELINE_SQL: &str = "WITH own_players AS (
        SELECT DISTINCT uuid FROM member_mc_links WHERE member_id = $1
     ), events AS (
        (SELECT 'admin:' || id AS key, ts AS at, 'admin_action' AS kind, actor, target,
                NULL::text AS player, action, NULL::text AS detail, details AS data
         FROM admin_actions WHERE target = $3 OR actor = $2)
        UNION ALL
        (SELECT 'strike:' || s.id, s.issued_at, 'strike_received', s.struck_by, $3, NULL, NULL, s.reason,
                jsonb_build_object('strike_id', s.id, 'expires_at', s.expires_at)
         FROM member_strike s WHERE s.member_id = $1)
        UNION ALL
        (SELECT 'unstrike:' || s.id, s.revoked_at, 'strike_revoked', s.revoked_by, $3, NULL, NULL, s.reason,
                jsonb_build_object('strike_id', s.id)
         FROM member_strike s WHERE s.member_id = $1 AND s.revoked_at IS NOT NULL)
        UNION ALL
        (SELECT 'issued:' || s.id, s.issued_at, 'strike_issued', s.struck_by, m.discord_id::text, NULL, NULL, s.reason,
                jsonb_build_object('strike_id', s.id, 'expires_at', s.expires_at)
         FROM member_strike s JOIN members m ON m.id = s.member_id
         WHERE s.struck_by = $2 AND s.member_id IS DISTINCT FROM $1)
        UNION ALL
        (SELECT 'own:' || e.id, e.ts, 'player_event', e.author, NULL, e.uuid, e.kind, e.reason,
                jsonb_build_object('tag_type', e.tag_type, 'expires_at', e.expires_at)
         FROM player_events e WHERE e.uuid IN (SELECT uuid FROM own_players))
        UNION ALL
        (SELECT 'authored:' || e.id, e.ts, 'tag_authored', e.author, NULL, e.uuid, e.kind, e.reason,
                jsonb_build_object('tag_type', e.tag_type)
         FROM player_events e
         WHERE e.author = $2 AND e.uuid NOT IN (SELECT uuid FROM own_players))
        UNION ALL
        (SELECT 'reviewed:' || e.id, e.ts, 'tag_reviewed', e.author, NULL, e.uuid, e.kind, e.reason,
                jsonb_build_object('tag_type', e.tag_type)
         FROM player_events e WHERE $2 = ANY(e.reviewed_by))
        UNION ALL
        (SELECT 'review:' || r.id, r.created_at, 'review_submitted', r.submitter_id, NULL, r.player_uuid, r.status, NULL,
                jsonb_build_object('review_id', r.id)
         FROM review r WHERE r.submitter_id = $2)
        UNION ALL
        (SELECT 'vote:' || v.id, v.cast_at, 'vote', v.voter_id, NULL, r.player_uuid, v.kind, v.reason,
                jsonb_build_object('review_id', v.review_id, 'disposition', v.disposition)
         FROM review_vote v JOIN review r ON r.id = v.review_id WHERE v.voter_id = $2)
        UNION ALL
        (SELECT 'key:' || k.id, k.created_at, 'key_created', NULL, $3, NULL, k.kind, NULL,
                jsonb_build_object('key_id', k.id, 'prefix', left(k.api_key, $6))
         FROM member_api_keys k WHERE k.member_id = $1 AND k.created_at IS NOT NULL)
        UNION ALL
        (SELECT 'retired:' || k.id, k.retired_at, 'key_retired', NULL, $3, NULL, k.kind, NULL,
                jsonb_build_object('key_id', k.id, 'prefix', left(k.api_key, $6))
         FROM member_api_keys k WHERE k.member_id = $1 AND k.retired_at IS NOT NULL)
        UNION ALL
        (SELECT 'link:' || l.id, l.linked_at, 'linked', NULL, $3, l.uuid, l.relation, NULL, NULL
         FROM member_mc_links l WHERE l.member_id = $1 AND l.linked_at IS NOT NULL)
        UNION ALL
        (SELECT 'unlink:' || l.id, l.unlinked_at, 'unlinked', NULL, $3, l.uuid, l.relation, NULL, NULL
         FROM member_mc_links l WHERE l.member_id = $1 AND l.unlinked_at IS NOT NULL)
     )
     SELECT key, at, kind, actor, target, player, action, detail, data FROM events
     WHERE (at, key) < ($4, $5)
     ORDER BY at DESC, key DESC
     LIMIT $7";

pub async fn timeline(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    query: Result<Query<TimelineQuery>, QueryRejection>,
) -> Result<Json<CursorPage<TimelineEvent>>, AdminError> {
    let Query(query) = query?;
    let limit = page_limit(query.limit)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(TimelineCursor::decode)
        .transpose()?;
    let target = AccountTarget::resolve(&state, &raw).await?;
    let start = cursor.unwrap_or_else(TimelineCursor::newest);
    let mut rows = read_events(state.db.pool(), target, &start, limit + 1).await?;
    let full = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = full
        .then(|| {
            rows.last().map(|row| {
                TimelineCursor {
                    at: row.at,
                    key: row.key.clone(),
                }
                .encode()
            })
        })
        .flatten();
    let items: Vec<TimelineEvent> = rows.into_iter().map(event).collect();
    let mentioned = items
        .iter()
        .flat_map(|event| [&event.actor, &event.target, &event.player])
        .flatten();
    let labels = state.names.cached(&label_request(mentioned)).await?;
    Ok(Json(CursorPage {
        items,
        next_cursor,
        labels,
    }))
}

async fn read_events(
    pool: &PgPool,
    target: AccountTarget,
    start: &TimelineCursor,
    fetch: usize,
) -> Result<Vec<EventRow>, AdminError> {
    sqlx::query_as(TIMELINE_SQL)
        .bind(target.member_id)
        .bind(target.discord_id)
        .bind(target.discord_id.to_string())
        .bind(start.at)
        .bind(&start.key)
        .bind(reqlog::KEY_PREFIX_CHARS as i32)
        .bind(fetch as i64)
        .fetch_all(pool)
        .await
        .context("read the account's timeline")
}

impl TimelineCursor {
    fn newest() -> Self {
        Self {
            at: DateTime::<Utc>::MAX_UTC,
            key: String::new(),
        }
    }

    fn encode(&self) -> String {
        format!(
            "{}{CURSOR_SEPARATOR}{}",
            self.at.timestamp_micros(),
            self.key
        )
    }

    fn decode(raw: &str) -> Result<Self, AdminError> {
        let invalid =
            || AdminError::BadRequest(format!("cursor {raw:?} is not a valid timeline cursor"));
        let (micros, key) = raw.split_once(CURSOR_SEPARATOR).ok_or_else(invalid)?;
        let micros: i64 = micros.parse().map_err(|_| invalid())?;
        Ok(Self {
            at: DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?,
            key: key.to_string(),
        })
    }
}

fn page_limit(requested: Option<usize>) -> Result<usize, AdminError> {
    let limit = requested.unwrap_or(TIMELINE_PAGE);
    if !(1..=MAX_TIMELINE_PAGE).contains(&limit) {
        return Err(AdminError::BadRequest(format!(
            "limit must be between 1 and {MAX_TIMELINE_PAGE}"
        )));
    }
    Ok(limit)
}

fn event(row: EventRow) -> TimelineEvent {
    let target = row
        .target
        .as_deref()
        .and_then(|text| parse_discord_id(text).ok())
        .map(account_ref);
    TimelineEvent {
        target_text: row.target.filter(|_| target.is_none()),
        target,
        key: row.key,
        at: row.at,
        kind: row.kind,
        actor: row.actor.map(account_ref),
        player: row.player.as_deref().map(player_ref),
        action: row.action,
        detail: row.detail,
        data: row.data.map_or(Value::Null, exact_integers),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::super::test_db::{at, member, pool, unique_key};
    use super::*;

    #[test]
    fn timeline_cursors_round_trip_and_reject_garbage() {
        let cursor = TimelineCursor {
            at: Utc.with_ymd_and_hms(2026, 10, 15, 1, 2, 3).unwrap(),
            key: "admin:42".into(),
        };
        assert_eq!(TimelineCursor::decode(&cursor.encode()).unwrap(), cursor);
        for raw in ["", "abc_admin:1", "17"] {
            assert_eq!(
                TimelineCursor::decode(raw).unwrap_err().code(),
                "bad_request",
                "{raw}"
            );
        }
    }

    #[test]
    fn snowflake_targets_become_accounts_and_other_targets_stay_text() {
        let row = |target: &str| EventRow {
            key: "admin:1".into(),
            at: Utc::now(),
            kind: "admin_action".into(),
            actor: Some(354_598_337_325_826_048),
            target: Some(target.into()),
            player: None,
            action: Some("extension".into()),
            detail: None,
            data: None,
        };
        let account = event(row("354598337325826048"));
        assert!(matches!(account.target, Some(EntityRef::Account { .. })));
        assert_eq!(account.target_text, None);
        let other = event(row("upstream/probe"));
        assert_eq!(other.target, None);
        assert_eq!(other.target_text.as_deref(), Some("upstream/probe"));
    }

    #[tokio::test]
    async fn the_timeline_pages_through_every_kind_of_event_newest_first() {
        let Some(pool) = pool().await else {
            return;
        };
        let key = unique_key();
        let target = member(&pool, &key).await;
        let member_id = target.member_id.unwrap();
        let player = uuid::Uuid::new_v4().simple().to_string();
        sqlx::query(
            "INSERT INTO admin_actions (actor, action, target, details, ts) VALUES (1, 'lock_member', $1, '{}', $2)",
        )
        .bind(target.discord_id.to_string())
        .bind(at(50))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO member_api_keys (member_id, kind, api_key, created_at, retired_at) VALUES ($1, 'personal', $2, $3, $4)",
        )
        .bind(member_id)
        .bind(unique_key())
        .bind(at(10))
        .bind(at(40))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO member_mc_links (member_id, uuid, relation, linked_at, unlinked_at) VALUES ($1, $2, 'alt', $3, $4)",
        )
        .bind(member_id)
        .bind(&player)
        .bind(at(20))
        .bind(at(30))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO player_events (uuid, kind, tag_type, author, ts) VALUES ($1, 'tag_set', 'sniper', 2, $2)",
        )
        .bind(&player)
        .bind(at(25))
        .execute(&pool)
        .await
        .unwrap();
        let mut seen = Vec::new();
        let mut cursor = TimelineCursor::newest();
        loop {
            let page = read_events(&pool, target, &cursor, 2).await.unwrap();
            let Some(last) = page.last() else {
                break;
            };
            cursor = TimelineCursor {
                at: last.at,
                key: last.key.clone(),
            };
            seen.extend(page.into_iter().map(|row| (row.at, row.kind)));
        }
        let kinds: Vec<&str> = seen.iter().map(|(_, kind)| kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "admin_action",
                "key_retired",
                "unlinked",
                "player_event",
                "linked",
                "key_created"
            ]
        );
        assert!(seen.windows(2).all(|pair| pair[0].0 >= pair[1].0));
    }
}
