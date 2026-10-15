use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use database::Database;
use reqlog::{KEY_PREFIX_CHARS, KeyKind};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{Account, Found, KeyMatch, ProbeContext, bounded, finish, open};
use crate::error::{AdminError, Context};

pub const KEY_PREFIX_MATCH_LIMIT: i64 = 10;
const UNMAPPED_KEY_KIND: KeyKind = KeyKind::Legacy;

static KEYS_BY_VALUE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT k.id AS key_id, k.kind, k.retired_at IS NOT NULL AS retired,
                m.id AS member_id, m.discord_id, left(k.api_key, {KEY_PREFIX_CHARS}) AS prefix
         FROM member_api_keys k JOIN members m ON m.id = k.member_id
         WHERE k.api_key = ANY($1)
         UNION ALL
         SELECT NULL, 'personal', false, m.id, m.discord_id, left(m.api_key, {KEY_PREFIX_CHARS})
         FROM members m
         WHERE m.api_key = ANY($1)
           AND NOT EXISTS (SELECT 1 FROM member_api_keys k WHERE k.api_key = m.api_key)
         UNION ALL
         SELECT NULL, 'developer', false, m.id, m.discord_id, left(d.api_key, {KEY_PREFIX_CHARS})
         FROM developer_keys d JOIN members m ON m.id = d.member_id
         WHERE d.api_key = ANY($1)
           AND NOT EXISTS (SELECT 1 FROM member_api_keys k WHERE k.api_key = d.api_key)"
    )
});
static KEYS_BY_PREFIX: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT k.id AS key_id, k.kind, k.retired_at IS NOT NULL AS retired,
                m.id AS member_id, m.discord_id, left(k.api_key, {KEY_PREFIX_CHARS}) AS prefix
         FROM member_api_keys k JOIN members m ON m.id = k.member_id
         WHERE left(k.api_key, {KEY_PREFIX_CHARS}) = $1
         ORDER BY k.retired_at IS NOT NULL, k.id DESC
         LIMIT $2"
    )
});

#[derive(FromRow)]
struct KeyRow {
    key_id: Option<i64>,
    kind: String,
    retired: bool,
    member_id: i64,
    discord_id: i64,
    prefix: String,
}

pub(super) async fn api_key(db: &Database, key: Uuid) -> Result<Vec<Found>, AdminError> {
    let forms = [key.hyphenated().to_string(), key.simple().to_string()];
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let rows: Vec<KeyRow> = sqlx::query_as(KEYS_BY_VALUE.as_str())
        .bind(&forms[..])
        .fetch_all(&mut *tx)
        .await
        .context("look up the API key")?;
    finish(tx).await?;
    Ok(rows
        .into_iter()
        .map(|row| Found::Key {
            key: key_match(row),
            exact: true,
        })
        .collect())
}

pub(super) async fn key_prefix(
    context: &ProbeContext<'_>,
    prefix: &str,
) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let stored = stored_keys(&mut tx, prefix).await?;
    let found = match (stored.is_empty(), context.coverage.rollups_from) {
        (true, Some(_)) => presented_prefix(&mut tx, prefix).await?,
        _ => stored,
    };
    finish(tx).await?;
    Ok(found)
}

async fn stored_keys(conn: &mut PgConnection, prefix: &str) -> Result<Vec<Found>, AdminError> {
    let rows: Vec<KeyRow> = sqlx::query_as(KEYS_BY_PREFIX.as_str())
        .bind(prefix)
        .bind(KEY_PREFIX_MATCH_LIMIT)
        .fetch_all(conn)
        .await
        .context("look up keys by prefix")?;
    Ok(rows
        .into_iter()
        .map(|row| Found::Key {
            key: key_match(row),
            exact: false,
        })
        .collect())
}

async fn presented_prefix(conn: &mut PgConnection, prefix: &str) -> Result<Vec<Found>, AdminError> {
    let presented: Option<(DateTime<Utc>, i16)> = sqlx::query_as(
        "SELECT hour, key_kind FROM api_caller_hour
         WHERE key_prefix = $1
         ORDER BY hour DESC LIMIT 1",
    )
    .bind(prefix)
    .fetch_optional(conn)
    .await
    .context("look for requests presenting the key prefix")?;
    Ok(presented
        .map(|(last_hour, code)| Found::UnknownPrefix {
            prefix: prefix.to_string(),
            kind: rolled_up_kind(code),
            last_hour,
        })
        .into_iter()
        .collect())
}

fn key_match(row: KeyRow) -> KeyMatch {
    KeyMatch {
        key_id: row.key_id,
        kind: KeyKind::from_label(&row.kind).unwrap_or_else(|| {
            tracing::warn!(kind = row.kind, "stored API key has an unknown kind");
            UNMAPPED_KEY_KIND
        }),
        owner: Account::member(row.discord_id, row.member_id),
        retired: row.retired,
        prefix: row.prefix,
    }
}

fn rolled_up_kind(code: i16) -> KeyKind {
    KeyKind::from_code(code).unwrap_or_else(|| {
        tracing::warn!(code, "rolled-up requests carry an unknown key kind");
        UNMAPPED_KEY_KIND
    })
}
