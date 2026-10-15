use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, TimeDelta, Utc};
use reqlog::{KEY_PREFIX_CHARS, KeyKind};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use crate::error::{AdminError, Context};
use crate::query::RequestFilter;
use crate::refs::{EntityRef, Labels, Page, RequestRow};
use crate::routes::ref_labels::label_request;
use crate::routes::streams::{StreamScope, request_stream};
use crate::serde_ts;
use crate::state::AppState;

const UNATTRIBUTED_WINDOW: TimeDelta = TimeDelta::days(14);
const UNATTRIBUTED_CAP: i64 = 1_000;
const KEY_COLUMNS: &str = "SELECT k.id AS key_id, k.kind, k.api_key, k.created_at, k.retired_at,
        m.discord_id, m.id AS member_id,
        CASE k.kind WHEN 'developer' THEN coalesce(d.locked, false) ELSE m.key_locked END AS locked
     FROM member_api_keys k
     JOIN members m ON m.id = k.member_id
     LEFT JOIN developer_keys d ON d.api_key = k.api_key";

#[derive(Serialize)]
pub struct KeyLookup {
    prefix: String,
    internal: bool,
    matches: Vec<KeyMatch>,
    unattributed: Unattributed,
    labels: Labels,
}

#[derive(Serialize)]
struct KeyMatch {
    key_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    prefix: String,
    account: EntityRef,
    kind: String,
    state: KeyState,
    locked: bool,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    created_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    retired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum KeyState {
    Current,
    Retired,
}

#[derive(Serialize)]
struct Unattributed {
    requests_14d: i64,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    last: Option<DateTime<Utc>>,
    capped: bool,
}

#[derive(FromRow)]
struct PrefixTraffic {
    requests_14d: i64,
    last: Option<DateTime<Utc>>,
    capped: bool,
    internal: bool,
}

#[derive(FromRow)]
struct KeyRow {
    key_id: i64,
    kind: String,
    api_key: String,
    created_at: Option<DateTime<Utc>>,
    retired_at: Option<DateTime<Utc>>,
    discord_id: i64,
    member_id: i64,
    locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disclosure {
    PrefixOnly,
    FullKey,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/id/{key_id}", get(by_id))
        .route("/{prefix}", get(by_prefix))
        .route("/{prefix}/requests", get(requests))
}

async fn by_prefix(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<KeyLookup>, AdminError> {
    let prefix = parse_prefix(&raw)?;
    let rows = keys_with_prefix(state.db.pool(), &prefix).await?;
    Ok(Json(
        lookup(&state, prefix, rows, Disclosure::PrefixOnly).await?,
    ))
}

async fn by_id(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<KeyLookup>, AdminError> {
    let key_id: i64 = raw
        .parse()
        .map_err(|_| AdminError::BadRequest(format!("{raw:?} is not a key id")))?;
    let row: KeyRow = sqlx::query_as(&format!("{KEY_COLUMNS} WHERE k.id = $1"))
        .bind(key_id)
        .fetch_optional(state.db.pool())
        .await
        .context("read the key")?
        .ok_or_else(|| AdminError::NotFound(format!("no key #{key_id}")))?;
    let prefix = prefix_of(&row.api_key);
    Ok(Json(
        lookup(&state, prefix, vec![row], Disclosure::FullKey).await?,
    ))
}

async fn requests(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    filter: Result<Query<RequestFilter>, QueryRejection>,
) -> Result<Json<Page<RequestRow>>, AdminError> {
    let Query(filter) = filter?;
    let prefix = parse_prefix(&raw)?;
    Ok(Json(
        request_stream(&state, StreamScope::KeyPrefix(prefix), filter).await?,
    ))
}

async fn lookup(
    state: &AppState,
    prefix: String,
    rows: Vec<KeyRow>,
    disclosure: Disclosure,
) -> Result<KeyLookup, AdminError> {
    let traffic = unattributed(state.db.pool(), &prefix).await?;
    let matches: Vec<KeyMatch> = rows
        .into_iter()
        .map(|row| key_match(row, disclosure))
        .collect();
    let labels = state
        .names
        .cached(&label_request(matches.iter().map(|found| &found.account)))
        .await?;
    Ok(KeyLookup {
        prefix,
        internal: traffic.internal,
        matches,
        unattributed: Unattributed {
            requests_14d: traffic.requests_14d,
            last: traffic.last,
            capped: traffic.capped,
        },
        labels,
    })
}

async fn keys_with_prefix(pool: &PgPool, prefix: &str) -> Result<Vec<KeyRow>, AdminError> {
    sqlx::query_as(&format!(
        "{KEY_COLUMNS} WHERE left(k.api_key, {KEY_PREFIX_CHARS}) = $1 ORDER BY k.retired_at IS NULL DESC, k.id DESC"
    ))
    .bind(prefix)
    .fetch_all(pool)
    .await
    .context("read the keys with this prefix")
}

async fn unattributed(pool: &PgPool, prefix: &str) -> Result<PrefixTraffic, AdminError> {
    sqlx::query_as(
        "SELECT count(*)::int8 AS requests_14d, max(ts) AS last, count(*) >= $3 AS capped,
                coalesce(bool_or(key_kind = $4), false) AS internal
         FROM (
            SELECT ts, key_kind FROM api_request_log
            WHERE key_prefix = $1 AND member_id IS NULL AND ts >= $2
            ORDER BY ts DESC LIMIT $3
         ) AS unattributed",
    )
    .bind(prefix)
    .bind(Utc::now() - UNATTRIBUTED_WINDOW)
    .bind(UNATTRIBUTED_CAP)
    .bind(KeyKind::Internal.label())
    .fetch_one(pool)
    .await
    .context("count the unattributed requests with this prefix")
}

fn parse_prefix(raw: &str) -> Result<String, AdminError> {
    let printable = raw.bytes().all(|byte| byte.is_ascii_graphic());
    if raw.len() != KEY_PREFIX_CHARS || !printable {
        return Err(AdminError::BadRequest(format!(
            "{raw:?} is not a key prefix; a prefix is the first {KEY_PREFIX_CHARS} characters of a key"
        )));
    }
    Ok(raw.to_string())
}

fn prefix_of(key: &str) -> String {
    key.chars().take(KEY_PREFIX_CHARS).collect()
}

fn key_match(row: KeyRow, disclosure: Disclosure) -> KeyMatch {
    KeyMatch {
        key_id: row.key_id,
        prefix: prefix_of(&row.api_key),
        key: (disclosure == Disclosure::FullKey).then_some(row.api_key),
        account: EntityRef::Account {
            discord_id: row.discord_id,
            member_id: Some(row.member_id),
        },
        kind: row.kind,
        state: match row.retired_at {
            Some(_) => KeyState::Retired,
            None => KeyState::Current,
        },
        locked: row.locked,
        created_at: row.created_at,
        retired_at: row.retired_at,
    }
}

#[cfg(test)]
mod tests {
    use super::super::accounts::test_db::{member, pool, unique_key};
    use super::*;

    fn row(retired_at: Option<DateTime<Utc>>) -> KeyRow {
        KeyRow {
            key_id: 9,
            kind: "personal".into(),
            api_key: "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d".into(),
            created_at: None,
            retired_at,
            discord_id: 354_598_337_325_826_048,
            member_id: 3,
            locked: false,
        }
    }

    #[test]
    fn a_prefix_is_exactly_eight_printable_characters() {
        assert_eq!(parse_prefix("1a2b3c4d").unwrap(), "1a2b3c4d");
        for raw in ["1a2b3c4", "1a2b3c4d5", "1a2b 3c4", ""] {
            assert_eq!(
                parse_prefix(raw).unwrap_err().code(),
                "bad_request",
                "{raw}"
            );
        }
    }

    #[test]
    fn only_a_lookup_by_id_discloses_the_full_key() {
        let by_prefix = key_match(row(None), Disclosure::PrefixOnly);
        assert_eq!(by_prefix.key, None);
        assert_eq!(by_prefix.prefix, "1a2b3c4d");
        assert_eq!(by_prefix.state, KeyState::Current);
        let by_id = key_match(row(Some(Utc::now())), Disclosure::FullKey);
        assert_eq!(
            by_id.key.as_deref(),
            Some("1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d")
        );
        assert_eq!(by_id.state, KeyState::Retired);
    }

    #[tokio::test]
    async fn the_prefix_of_the_internal_key_is_recognised_from_its_requests() {
        let Some(pool) = pool().await else {
            return;
        };
        let prefix = prefix_of(&unique_key());
        for key_kind in [KeyKind::Internal.label(), "key"] {
            sqlx::query(
                "INSERT INTO api_request_log (ts, key_prefix, key_kind, method, path, status, latency_ms)
                 VALUES (now(), $1, $2, 'GET', '/v3/player/tags', 200, 1)",
            )
            .bind(&prefix)
            .bind(key_kind)
            .execute(&pool)
            .await
            .unwrap();
        }
        let traffic = unattributed(&pool, &prefix).await.unwrap();
        assert!(traffic.internal);
        assert_eq!(traffic.requests_14d, 2);
        assert!(keys_with_prefix(&pool, &prefix).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn only_requests_no_account_claimed_count_as_unattributed() {
        let Some(pool) = pool().await else {
            return;
        };
        let key = unique_key();
        let target = member(&pool, &key).await;
        sqlx::query(
            "INSERT INTO member_api_keys (member_id, kind, api_key) VALUES ($1, 'personal', $2)",
        )
        .bind(target.member_id)
        .bind(&key)
        .execute(&pool)
        .await
        .unwrap();
        let prefix = prefix_of(&key);
        for member_id in [None, None, None, target.member_id] {
            sqlx::query(
                "INSERT INTO api_request_log (ts, key_prefix, member_id, method, path, status, latency_ms)
                 VALUES (now(), $1, $2, 'GET', '/v3/player/tags', 429, 1)",
            )
            .bind(&prefix)
            .bind(member_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        let counted = unattributed(&pool, &prefix).await.unwrap();
        assert_eq!(counted.requests_14d, 3);
        assert!(!counted.capped);
        assert!(!counted.internal);
        let keys = keys_with_prefix(&pool, &prefix).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(
            key_match(keys.into_iter().next().unwrap(), Disclosure::PrefixOnly).state,
            KeyState::Current
        );
    }
}
