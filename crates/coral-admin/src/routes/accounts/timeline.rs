use std::sync::LazyLock;

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
const KIND_SEPARATOR: char = ',';

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    kinds: Option<String>,
    from: Option<DateTime<Utc>>,
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

/// One source of timeline events: its kind, its unique key and time, the remaining
/// columns, and the rows it reads from.
struct Branch {
    kind: &'static str,
    key: &'static str,
    at: &'static str,
    columns: &'static str,
    source: &'static str,
}

const BRANCHES: &[Branch] = &[
    Branch {
        kind: "admin_action",
        key: "'admin:' || id",
        at: "ts",
        columns: "actor, target, NULL::text, action, NULL::text, details",
        source: "admin_actions WHERE (target = $3 OR actor = $2)",
    },
    Branch {
        kind: "strike_received",
        key: "'strike:' || s.id",
        at: "s.issued_at",
        columns: "s.struck_by, $3, NULL, NULL, s.reason, jsonb_build_object('strike_id', s.id, 'expires_at', s.expires_at)",
        source: "member_strike s WHERE s.member_id = $1",
    },
    Branch {
        kind: "strike_revoked",
        key: "'unstrike:' || s.id",
        at: "s.revoked_at",
        columns: "s.revoked_by, $3, NULL, NULL, s.reason, jsonb_build_object('strike_id', s.id)",
        source: "member_strike s WHERE s.member_id = $1 AND s.revoked_at IS NOT NULL",
    },
    Branch {
        kind: "strike_issued",
        key: "'issued:' || s.id",
        at: "s.issued_at",
        columns: "s.struck_by, m.discord_id::text, NULL, NULL, s.reason, jsonb_build_object('strike_id', s.id, 'expires_at', s.expires_at)",
        source: "member_strike s JOIN members m ON m.id = s.member_id WHERE s.struck_by = $2 AND s.member_id IS DISTINCT FROM $1",
    },
    Branch {
        kind: "player_event",
        key: "'own:' || e.id",
        at: "e.ts",
        columns: "e.author, NULL, e.uuid, e.kind, e.reason, jsonb_build_object('tag_type', e.tag_type, 'expires_at', e.expires_at)",
        source: "player_events e WHERE e.uuid IN (SELECT uuid FROM member_mc_links WHERE member_id = $1)",
    },
    Branch {
        kind: "tag_authored",
        key: "'authored:' || e.id",
        at: "e.ts",
        columns: "e.author, NULL, e.uuid, e.kind, e.reason, jsonb_build_object('tag_type', e.tag_type, 'expires_at', e.expires_at,
            'ended_by', CASE
                WHEN e.kind <> 'tag_set' THEN NULL
                WHEN e.expires_at <= COALESCE(later.ts, now()) THEN 'expired'
                WHEN later.id IS NULL THEN NULL
                WHEN later.kind = 'tag_set' OR EXISTS (
                    SELECT 1 FROM player_events paired
                    WHERE paired.uuid = e.uuid AND paired.kind = 'tag_set'
                      AND paired.ts = later.ts AND paired.id > later.id) THEN 'replaced'
                ELSE 'cleared'
            END)",
        source: "player_events e
            LEFT JOIN LATERAL (SELECT later.id, later.kind, later.ts FROM player_events later
                               WHERE later.uuid = e.uuid AND later.tag_type = e.tag_type
                                 AND later.kind IN ('tag_set', 'tag_clear') AND (later.ts, later.id) > (e.ts, e.id)
                               ORDER BY later.ts, later.id LIMIT 1) later ON true
            WHERE e.author = $2 AND e.uuid NOT IN (SELECT uuid FROM member_mc_links WHERE member_id = $1)",
    },
    Branch {
        kind: "tag_reviewed",
        key: "'reviewed:' || e.id",
        at: "e.ts",
        columns: "e.author, NULL, e.uuid, e.kind, e.reason, jsonb_build_object('tag_type', e.tag_type)",
        source: "player_events e WHERE e.reviewed_by @> ARRAY[$2::int8]",
    },
    Branch {
        kind: "review_submitted",
        key: "'review:' || r.id",
        at: "r.created_at",
        columns: "r.submitter_id, NULL, r.player_uuid, r.status, NULL, jsonb_build_object('review_id', r.id)",
        source: "review r WHERE r.submitter_id = $2",
    },
    Branch {
        kind: "vote",
        key: "'vote:' || v.id",
        at: "v.cast_at",
        columns: "v.voter_id, NULL, r.player_uuid, v.kind, v.reason, jsonb_build_object('review_id', v.review_id, 'disposition', v.disposition)",
        source: "review_vote v JOIN review r ON r.id = v.review_id WHERE v.voter_id = $2",
    },
    Branch {
        kind: "key_created",
        key: "'key:' || k.id",
        at: "k.created_at",
        columns: "NULL, $3, NULL, k.kind, NULL, jsonb_build_object('key_id', k.id, 'prefix', left(k.api_key, $6))",
        source: "member_api_keys k WHERE k.member_id = $1 AND k.created_at IS NOT NULL",
    },
    Branch {
        kind: "key_retired",
        key: "'retired:' || k.id",
        at: "k.retired_at",
        columns: "NULL, $3, NULL, k.kind, NULL, jsonb_build_object('key_id', k.id, 'prefix', left(k.api_key, $6))",
        source: "member_api_keys k WHERE k.member_id = $1 AND k.retired_at IS NOT NULL",
    },
    Branch {
        kind: "linked",
        key: "'link:' || l.id",
        at: "l.linked_at",
        columns: "NULL, $3, l.uuid, l.relation, NULL, NULL",
        source: "member_mc_links l WHERE l.member_id = $1 AND l.linked_at IS NOT NULL",
    },
    Branch {
        kind: "unlinked",
        key: "'unlink:' || l.id",
        at: "l.unlinked_at",
        columns: "NULL, $3, l.uuid, l.relation, NULL, NULL",
        source: "member_mc_links l WHERE l.member_id = $1 AND l.unlinked_at IS NOT NULL",
    },
];

/// Each branch sorts and limits itself below the cursor, so a page reads at most
/// `limit` rows per kind instead of the account's whole history.
static TIMELINE_SQL: LazyLock<String> = LazyLock::new(|| {
    let branches: Vec<String> = BRANCHES.iter().map(Branch::sql).collect();
    format!(
        "SELECT key, at, kind, actor, target, player, action, detail, data FROM ({}) AS events(key, at, kind, actor, target, player, action, detail, data)
         ORDER BY at DESC, key DESC
         LIMIT $7",
        branches.join(" UNION ALL ")
    )
});

pub async fn timeline(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    query: Result<Query<TimelineQuery>, QueryRejection>,
) -> Result<Json<CursorPage<TimelineEvent>>, AdminError> {
    let Query(query) = query?;
    let limit = page_limit(query.limit)?;
    let kinds = query.kinds.as_deref().map(parse_kinds).transpose()?;
    let cursor = query
        .cursor
        .as_deref()
        .map(TimelineCursor::decode)
        .transpose()?;
    let target = AccountTarget::resolve(&state, &raw).await?;
    let start = cursor.unwrap_or_else(TimelineCursor::newest);
    let mut rows = read_events(
        state.db.pool(),
        target,
        &start,
        kinds.as_deref(),
        query.from,
        limit + 1,
    )
    .await?;
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
    kinds: Option<&[String]>,
    from: Option<DateTime<Utc>>,
    fetch: usize,
) -> Result<Vec<EventRow>, AdminError> {
    sqlx::query_as(&TIMELINE_SQL)
        .bind(target.member_id)
        .bind(target.discord_id)
        .bind(target.discord_id.to_string())
        .bind(start.at)
        .bind(&start.key)
        .bind(reqlog::KEY_PREFIX_CHARS as i32)
        .bind(fetch as i64)
        .bind(kinds)
        .bind(from)
        .fetch_all(pool)
        .await
        .context("read the account's timeline")
}

/// Reads a comma-separated list of event kinds, rejecting any the timeline doesn't have.
fn parse_kinds(raw: &str) -> Result<Vec<String>, AdminError> {
    raw.split(KIND_SEPARATOR)
        .map(|kind| {
            BRANCHES
                .iter()
                .any(|branch| branch.kind == kind)
                .then(|| kind.to_string())
                .ok_or_else(|| {
                    AdminError::BadRequest(format!("{kind:?} is not a timeline event kind"))
                })
        })
        .collect()
}

impl Branch {
    fn sql(&self) -> String {
        let Self {
            kind,
            key,
            at,
            columns,
            source,
        } = self;
        format!(
            "(SELECT {key} AS key, {at} AS at, '{kind}' AS kind, {columns}
              FROM {source}
                AND ($8::text[] IS NULL OR '{kind}' = ANY($8))
                AND {at} <= $4 AND ({at}, {key}) < ($4, $5)
                AND ($9::timestamptz IS NULL OR {at} >= $9)
              ORDER BY {at} DESC, {key} DESC
              LIMIT $7)"
        )
    }
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
    fn kind_filters_accept_known_kinds_and_reject_others() {
        assert_eq!(parse_kinds("linked,vote").unwrap(), vec!["linked", "vote"]);
        assert!(parse_kinds("linked,nonsense").is_err());
    }

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
            let page = read_events(&pool, target, &cursor, None, None, 2)
                .await
                .unwrap();
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

    #[tokio::test]
    async fn authored_tags_say_how_they_ended() {
        let Some(pool) = pool().await else {
            return;
        };
        let target = member(&pool, &unique_key()).await;
        let author = target.discord_id;
        let other = author + 1;
        let [active, removed, overwritten, retyped, lapsed, retagged] =
            [(); 6].map(|()| uuid::Uuid::new_v4().simple().to_string());
        let insert = |uuid: &str,
                      kind: &str,
                      tag_type: &str,
                      by: i64,
                      minute: i64,
                      expires: Option<i64>| {
            sqlx::query(
                "INSERT INTO player_events (uuid, kind, tag_type, author, ts, expires_at) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(uuid.to_string())
            .bind(kind.to_string())
            .bind(tag_type.to_string())
            .bind(by)
            .bind(at(minute))
            .bind(expires.map(at))
            .execute(&pool)
        };
        insert(&active, "tag_set", "sniper", author, 1, None)
            .await
            .unwrap();
        insert(&removed, "tag_set", "sniper", author, 2, None)
            .await
            .unwrap();
        insert(&removed, "tag_clear", "sniper", other, 3, None)
            .await
            .unwrap();
        insert(&overwritten, "tag_set", "sniper", author, 4, None)
            .await
            .unwrap();
        insert(&overwritten, "tag_clear", "sniper", other, 5, None)
            .await
            .unwrap();
        insert(&overwritten, "tag_set", "sniper", other, 5, None)
            .await
            .unwrap();
        insert(&retyped, "tag_set", "sniper", author, 6, None)
            .await
            .unwrap();
        insert(&retyped, "tag_clear", "sniper", other, 7, None)
            .await
            .unwrap();
        insert(&retyped, "tag_set", "closet_cheater", other, 7, None)
            .await
            .unwrap();
        insert(&lapsed, "tag_set", "sniper", author, 8, Some(9))
            .await
            .unwrap();
        insert(&retagged, "tag_set", "sniper", author, 10, Some(11))
            .await
            .unwrap();
        insert(&retagged, "tag_set", "sniper", other, 12, None)
            .await
            .unwrap();
        let kinds = vec!["tag_authored".to_string()];
        let rows = read_events(
            &pool,
            target,
            &TimelineCursor::newest(),
            Some(&kinds),
            None,
            50,
        )
        .await
        .unwrap();
        let ended = |uuid: &str| {
            rows.iter()
                .find(|row| row.player.as_deref() == Some(uuid))
                .and_then(|row| {
                    row.data
                        .as_ref()?
                        .get("ended_by")?
                        .as_str()
                        .map(str::to_string)
                })
        };
        assert_eq!(ended(&active), None);
        assert_eq!(ended(&removed).as_deref(), Some("cleared"));
        assert_eq!(ended(&overwritten).as_deref(), Some("replaced"));
        assert_eq!(ended(&retyped).as_deref(), Some("replaced"));
        assert_eq!(ended(&lapsed).as_deref(), Some("expired"));
        assert_eq!(ended(&retagged).as_deref(), Some("expired"));
    }
}
