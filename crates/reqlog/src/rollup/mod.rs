use std::ops::Add;
use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::TIME_ORDERED_VERSION;
use crate::entry::KeyKind;
use crate::subjects::Form;

mod network;
pub(crate) mod rows;
mod span;
mod tables;
#[cfg(test)]
mod tests;

pub use network::{NETWORK_FOLD, advance_network};
pub use span::{HOUR, MINUTE, Span, ceil_to, floor_to};
pub use tables::{CALLER_HOUR, ROLLUP_TABLES, RollupTable, TRAFFIC_HOUR, TRAFFIC_MINUTE};

pub const ROLLUP_LOCK_KEY: i64 = 7_210_003;
pub const TRAFFIC_PASS_INTERVAL: Duration = Duration::from_secs(30);
pub const CALLER_PASS_INTERVAL: Duration = Duration::from_secs(120);
pub const NETWORK_PASS_INTERVAL: Duration = Duration::from_secs(60);
pub const TOP_PLAYERS_INTERVAL: Duration = Duration::from_secs(300);
pub const PRUNE_INTERVAL: Duration = Duration::from_secs(3600);
pub const REROLL_WINDOW: TimeDelta = TimeDelta::minutes(10);
pub const SETTLE: TimeDelta = TimeDelta::seconds(60);
pub const NETWORK_SETTLE: TimeDelta = TimeDelta::minutes(10);
pub const CATCH_UP_CHUNK: TimeDelta = TimeDelta::hours(1);
pub const TRAFFIC_MINUTE_RETENTION_DAYS: i64 = 30;
pub const TRAFFIC_HOUR_RETENTION_DAYS: i64 = 400;
pub const CALLER_RETENTION_DAYS: i64 = 400;
pub const PRUNE_BATCH: i64 = 10_000;
pub const LATENCY_BUCKET_BOUNDS_MS: [i32; 11] = [
    10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 30_000,
];
pub const TRAFFIC_WATERMARK: &str = "traffic";
pub const CALLER_WATERMARK: &str = "callers";
pub const NETWORK_WATERMARK: &str = "network";
pub const ROLLUPS_FEATURE: &str = "rollups";
pub const NETWORK_FEATURE: &str = "network";

const LOG_TABLE: &str = "api_request_log";
const MINUTE_BIN: &str = "1 minute";
const HOUR_BIN: &str = "1 hour";
const COVERAGE_NOTE: &str = "the rollup job's first processed minute";
const CAUGHT_UP_NOTE: &str = "the oldest retained log when the rollup job first caught up";
const CAUGHT_UP_WATERMARKS: [&str; 2] = [TRAFFIC_WATERMARK, CALLER_WATERMARK];

static TRAFFIC_MINUTE_REWRITE: LazyLock<String> = LazyLock::new(|| {
    TRAFFIC_MINUTE.rewrite(&format!(
        "{rows},
produced AS (
  SELECT {minute} AS minute, r.route, r.method, r.status, r.cause,
         r.key_kind, count(*)::int AS requests, sum(r.latency_ms)::bigint AS latency_sum_ms,
         max(r.latency_ms) AS latency_max_ms, ARRAY[{buckets}]::int[] AS latency_buckets
  FROM rolled r
  GROUP BY 1, 2, 3, 4, 5, 6
)",
        rows = rows::rolled_rows(),
        minute = rows::time_bin(MINUTE_BIN, "r.ts"),
        buckets =
            bucket_columns(|bucket| format!("count(*) FILTER (WHERE r.latency_bucket = {bucket})")),
    ))
});

static TRAFFIC_HOUR_REWRITE: LazyLock<String> = LazyLock::new(|| {
    TRAFFIC_HOUR.rewrite(&format!(
        "produced AS (
  SELECT {hour} AS hour, m.route, m.method, m.status, m.cause,
         m.key_kind, sum(m.requests)::int AS requests,
         sum(m.latency_sum_ms)::bigint AS latency_sum_ms, max(m.latency_max_ms) AS latency_max_ms,
         ARRAY[{buckets}]::int[] AS latency_buckets
  FROM api_traffic_minute m
  WHERE m.minute >= $1 AND m.minute < $2
  GROUP BY 1, 2, 3, 4, 5, 6
)",
        hour = rows::time_bin(HOUR_BIN, "m.minute"),
        buckets = bucket_columns(|bucket| format!("sum(m.latency_buckets[{}])", bucket + 1)),
    ))
});

static CALLER_HOUR_REWRITE: LazyLock<String> = LazyLock::new(|| {
    CALLER_HOUR.rewrite(&format!(
        "{rows},
produced AS (
  SELECT {hour} AS hour, r.key_kind, r.member_id, r.actor_id,
         r.key_prefix, r.service, r.route, r.method, r.status, r.cause,
         count(*)::int AS requests, sum(r.latency_ms)::bigint AS latency_sum_ms,
         sum(r.subjects)::bigint AS subjects
  FROM rolled r
  GROUP BY 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
)",
        rows = rows::rolled_rows(),
        hour = rows::time_bin(HOUR_BIN, "r.ts"),
    ))
});

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribution {
    internal_prefix: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, FromRow)]
pub struct Rewritten {
    pub written: i64,
    pub removed: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rolled {
    pub span: Span,
    pub rewritten: Rewritten,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyRange {
    pub ids: Vec<i64>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TopPlayer {
    pub player: Uuid,
    pub requests: i64,
    pub last_seen: DateTime<Utc>,
}

#[derive(FromRow)]
struct DirtyRow {
    id: i64,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
}

impl Attribution {
    pub async fn recorded(conn: &mut PgConnection) -> Result<Self, sqlx::Error> {
        let internal_prefix = sqlx::query_scalar(
            "SELECT key_prefix FROM api_request_log
             WHERE key_kind = $1 AND key_prefix IS NOT NULL
               AND uuid_extract_version(request_id) = $2
             ORDER BY ts DESC
             LIMIT 1",
        )
        .bind(KeyKind::Internal.label())
        .bind(TIME_ORDERED_VERSION as i16)
        .fetch_optional(conn)
        .await?;
        Ok(Self { internal_prefix })
    }

    pub fn with_internal_prefix(prefix: impl Into<String>) -> Self {
        Self {
            internal_prefix: Some(prefix.into()),
        }
    }

    pub fn without_internal_key() -> Self {
        Self::default()
    }

    pub fn internal_prefix(&self) -> Option<&str> {
        self.internal_prefix.as_deref()
    }
}

impl Add for Rewritten {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            written: self.written + other.written,
            removed: self.removed + other.removed,
        }
    }
}

pub async fn take_lock(conn: &mut PgConnection) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(ROLLUP_LOCK_KEY)
        .fetch_one(conn)
        .await
}

pub async fn wait_for_lock(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ROLLUP_LOCK_KEY)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn roll_traffic(
    conn: &mut PgConnection,
    span: Span,
    attribution: &Attribution,
) -> Result<Option<Rolled>, sqlx::Error> {
    let Some(span) = retained(conn, span.aligned(MINUTE)).await? else {
        return Ok(None);
    };
    let minutes = rewrite(conn, &TRAFFIC_MINUTE_REWRITE, span, Some(attribution)).await?;
    let hours = rewrite(conn, &TRAFFIC_HOUR_REWRITE, span.aligned(HOUR), None).await?;
    Ok(Some(Rolled {
        span,
        rewritten: minutes + hours,
    }))
}

pub async fn roll_callers(
    conn: &mut PgConnection,
    span: Span,
    attribution: &Attribution,
) -> Result<Option<Rolled>, sqlx::Error> {
    let Some(span) = retained(conn, span.aligned(HOUR)).await? else {
        return Ok(None);
    };
    let rewritten = rewrite(conn, &CALLER_HOUR_REWRITE, span, Some(attribution)).await?;
    Ok(Some(Rolled { span, rewritten }))
}

pub async fn log_floor(conn: &mut PgConnection) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT min(to_date(right(c.relname, 8), 'YYYYMMDD'))::timestamp AT TIME ZONE 'UTC'
         FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
         WHERE i.inhparent = $1::regclass AND c.relname ~ $2",
    )
    .bind(LOG_TABLE)
    .bind(format!("^{LOG_TABLE}_[0-9]{{8}}$"))
    .fetch_one(conn)
    .await
}

pub async fn watermark(
    conn: &mut PgConnection,
    name: &str,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar("SELECT watermark FROM api_rollup_state WHERE name = $1")
        .bind(name)
        .fetch_optional(conn)
        .await
}

pub async fn set_watermark(
    conn: &mut PgConnection,
    name: &str,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO api_rollup_state (name, watermark) VALUES ($1, $2)
         ON CONFLICT (name) DO UPDATE SET watermark = excluded.watermark",
    )
    .bind(name)
    .bind(at)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn record_coverage(
    conn: &mut PgConnection,
    feature: &str,
    since: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO api_coverage (feature, since, note) VALUES ($1, $2, $3)
         ON CONFLICT (feature) DO NOTHING",
    )
    .bind(feature)
    .bind(since)
    .bind(COVERAGE_NOTE)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn record_caught_up_coverage(
    conn: &mut PgConnection,
    settled: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let Some(floor) = log_floor(conn).await? else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO api_coverage (feature, since, note)
         SELECT $1, $2, $3
         WHERE (SELECT count(*) FROM api_rollup_state WHERE name = ANY($4) AND watermark >= $5)
               = cardinality($4)
         ON CONFLICT (feature) DO NOTHING",
    )
    .bind(ROLLUPS_FEATURE)
    .bind(floor)
    .bind(CAUGHT_UP_NOTE)
    .bind(&CAUGHT_UP_WATERMARKS[..])
    .bind(settled)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn dirty_ranges(
    conn: &mut PgConnection,
    limit: i64,
) -> Result<Vec<DirtyRange>, sqlx::Error> {
    let rows: Vec<DirtyRow> = sqlx::query_as(
        "SELECT id, from_ts, to_ts FROM api_rollup_dirty ORDER BY from_ts, id LIMIT $1",
    )
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(merge_dirty(rows))
}

pub async fn clear_dirty(conn: &mut PgConnection, ids: &[i64]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM api_rollup_dirty WHERE id = ANY($1)")
        .bind(ids)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn top_players(
    conn: &mut PgConnection,
    span: Span,
    limit: i64,
) -> Result<Vec<TopPlayer>, sqlx::Error> {
    sqlx::query_as(
        "SELECT player, count(*) AS requests, max(ts) AS last_seen
         FROM api_request_subject
         WHERE ts >= $1 AND ts < $2 AND player IS NOT NULL AND form <> $3
         GROUP BY player
         ORDER BY requests DESC, player
         LIMIT $4",
    )
    .bind(span.from)
    .bind(span.to)
    .bind(Form::Body.code())
    .bind(limit)
    .fetch_all(conn)
    .await
}

pub async fn prune(
    conn: &mut PgConnection,
    table: &RollupTable,
    now: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let removed = sqlx::query(&table.prune())
        .bind(now - table.retention())
        .bind(PRUNE_BATCH)
        .execute(conn)
        .await?;
    Ok(removed.rows_affected())
}

async fn retained(conn: &mut PgConnection, span: Span) -> Result<Option<Span>, sqlx::Error> {
    sqlx::query(&format!("LOCK TABLE ONLY {LOG_TABLE} IN ACCESS SHARE MODE"))
        .execute(&mut *conn)
        .await?;
    let floor = log_floor(conn).await?;
    Ok(floor.and_then(|floor| span.starting_no_earlier_than(floor)))
}

async fn rewrite(
    conn: &mut PgConnection,
    statement: &str,
    span: Span,
    attribution: Option<&Attribution>,
) -> Result<Rewritten, sqlx::Error> {
    let query = sqlx::query_as(statement).bind(span.from).bind(span.to);
    let query = match attribution {
        Some(attribution) => query.bind(attribution.internal_prefix()),
        None => query,
    };
    query.fetch_one(conn).await
}

fn bucket_columns(column: impl Fn(usize) -> String) -> String {
    (0..rows::latency_bucket_count())
        .map(column)
        .collect::<Vec<_>>()
        .join(", ")
}

fn merge_dirty(rows: Vec<DirtyRow>) -> Vec<DirtyRange> {
    let mut merged: Vec<DirtyRange> = Vec::new();
    for row in rows {
        let span = Span {
            from: floor_to(row.from_ts, HOUR),
            to: floor_to(row.to_ts.max(row.from_ts), HOUR) + HOUR,
        };
        match merged.last_mut() {
            Some(last) if last.span.touches(span) => {
                last.span = last.span.joined(span);
                last.ids.push(row.id);
            }
            _ => merged.push(DirtyRange {
                ids: vec![row.id],
                span,
            }),
        }
    }
    merged
}
