use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{PgConnection, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::entry::LogEntry;
use crate::rollup::{NETWORK_FOLD, NETWORK_WATERMARK};
use crate::subjects::Subject;
use crate::{SubjectSources, TIME_ORDERED_VERSION, WRITER_STATEMENT_TIMEOUT_MS};

const WRITER_SEEN: &str = "writer_seen";
const ATTRIBUTION_FEATURE: &str = "attribution";
const SUBJECTS_FEATURE: &str = "subjects";
const TIMESTAMP_RESOLUTION: TimeDelta = TimeDelta::microseconds(1);

pub struct PreparedEntry {
    pub entry: LogEntry,
    pub subjects: Option<Vec<Subject>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SubjectOwner<'a> {
    pub ts: DateTime<Utc>,
    pub request_id: Uuid,
    pub member_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub key_prefix: Option<&'a str>,
    pub key_kind: i16,
    pub service: i16,
    pub status: i16,
}

#[derive(Default)]
pub(crate) struct SubjectColumns {
    ts: Vec<DateTime<Utc>>,
    request_id: Vec<Uuid>,
    member_id: Vec<Option<i64>>,
    actor_id: Vec<Option<i64>>,
    key_prefix: Vec<Option<String>>,
    key_kind: Vec<i16>,
    service: Vec<i16>,
    status: Vec<i16>,
    form: Vec<i16>,
    player: Vec<Option<Uuid>>,
    name: Vec<Option<String>>,
}

impl PreparedEntry {
    fn subject_count(&self) -> Option<i16> {
        self.subjects.as_ref().map(|subjects| subjects.len() as i16)
    }
}

impl SubjectColumns {
    pub(crate) fn push(&mut self, owner: &SubjectOwner, subject: &Subject) {
        self.ts.push(owner.ts);
        self.request_id.push(owner.request_id);
        self.member_id.push(owner.member_id);
        self.actor_id.push(owner.actor_id);
        self.key_prefix.push(owner.key_prefix.map(str::to_owned));
        self.key_kind.push(owner.key_kind);
        self.service.push(owner.service);
        self.status.push(owner.status);
        self.form.push(subject.form.code());
        self.player.push(subject.player);
        self.name.push(subject.name.clone());
    }

    fn collect(entries: &[PreparedEntry]) -> Self {
        let mut columns = Self::default();
        for prepared in entries {
            let entry = &prepared.entry;
            let owner = SubjectOwner {
                ts: entry.ts,
                request_id: entry.request_id,
                member_id: entry.caller.member_id,
                actor_id: entry.caller.actor_id,
                key_prefix: entry.subject_key_prefix(),
                key_kind: entry.caller.key_kind.code(),
                service: entry.service().code(),
                status: status_code(entry.semantic_status()),
            };
            for subject in prepared.subjects.iter().flatten() {
                columns.push(&owner, subject);
            }
        }
        columns
    }
}

pub async fn limit_statement_time(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "SET LOCAL statement_timeout = {WRITER_STATEMENT_TIMEOUT_MS}"
    ))
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_log(
    conn: &mut PgConnection,
    entries: &[PreparedEntry],
) -> Result<(), sqlx::Error> {
    let mut query: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO api_request_log (request_id, ts, ip, country, member_id, actor_id, key_prefix, \
         key_kind, method, path, route, query, user_agent, status, outcome_status, cause, \
         latency_ms, error, subjects) ",
    );
    query.push_values(entries, |mut row, prepared| {
        let entry = &prepared.entry;
        row.push_bind(entry.request_id)
            .push_bind(entry.ts)
            .push_bind(entry.client.ip.map(|ip| ip.to_string()))
            .push_bind(entry.client.country.as_deref())
            .push_bind(entry.caller.member_id)
            .push_bind(entry.caller.actor_id)
            .push_bind(entry.client.key_prefix.as_deref())
            .push_bind(entry.caller.key_kind.label())
            .push_bind(entry.method.as_str())
            .push_bind(entry.path.as_str())
            .push_bind(entry.route.as_deref())
            .push_bind(entry.query.as_deref())
            .push_bind(entry.client.user_agent.as_deref())
            .push_bind(status_code(entry.status))
            .push_bind(entry.outcome_status.map(status_code))
            .push_bind(entry.cause.code())
            .push_bind(entry.latency_ms)
            .push_bind(entry.error.as_deref())
            .push_bind(prepared.subject_count());
    });
    query.build().execute(conn).await?;
    Ok(())
}

pub async fn insert_detail(
    conn: &mut PgConnection,
    entries: &[PreparedEntry],
) -> Result<(), sqlx::Error> {
    let mut query: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO api_request_detail (request_id, ts, member_id, headers, body, body_bytes, \
         body_complete, server_error) ",
    );
    query.push_values(entries, |mut row, prepared| {
        let entry = &prepared.entry;
        row.push_bind(entry.request_id)
            .push_bind(entry.ts)
            .push_bind(entry.caller.member_id)
            .push_bind(&entry.detail.headers)
            .push_bind(entry.detail.body.as_deref())
            .push_bind(entry.detail.body_bytes)
            .push_bind(entry.detail.body_complete)
            .push_bind(entry.detail.server_error.as_deref());
    });
    query.build().execute(conn).await?;
    Ok(())
}

pub async fn insert_subjects(
    conn: &mut PgConnection,
    entries: &[PreparedEntry],
) -> Result<(), sqlx::Error> {
    insert_subject_columns(conn, SubjectColumns::collect(entries)).await
}

pub(crate) async fn insert_subject_columns(
    conn: &mut PgConnection,
    columns: SubjectColumns,
) -> Result<(), sqlx::Error> {
    if columns.ts.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO api_request_subject (ts, request_id, member_id, actor_id, key_prefix, \
         key_kind, service, status, form, player, name)
         SELECT * FROM UNNEST($1::timestamptz[], $2::uuid[], $3::int8[], $4::int8[], $5::text[], \
         $6::int2[], $7::int2[], $8::int2[], $9::int2[], $10::uuid[], $11::text[])",
    )
    .bind(columns.ts)
    .bind(columns.request_id)
    .bind(columns.member_id)
    .bind(columns.actor_id)
    .bind(columns.key_prefix)
    .bind(columns.key_kind)
    .bind(columns.service)
    .bind(columns.status)
    .bind(columns.form)
    .bind(columns.player)
    .bind(columns.name)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn mark_late_rows(
    conn: &mut PgConnection,
    entries: &[PreparedEntry],
) -> Result<(), sqlx::Error> {
    let timestamps = entries.iter().map(|prepared| prepared.entry.ts);
    let (Some(from), Some(to)) = (timestamps.clone().min(), timestamps.max()) else {
        return Ok(());
    };
    sqlx::query("INSERT INTO api_rollup_dirty (from_ts, to_ts) VALUES ($1, $2)")
        .bind(from)
        .bind(to)
        .execute(&mut *conn)
        .await?;
    let watermark: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT watermark FROM api_rollup_state WHERE name = $1 FOR SHARE")
            .bind(NETWORK_WATERMARK)
            .fetch_optional(&mut *conn)
            .await?;
    let Some(watermark) = watermark else {
        return Ok(());
    };
    let folded: Vec<&LogEntry> = entries
        .iter()
        .map(|prepared| &prepared.entry)
        .filter(|entry| entry.ts < watermark)
        .collect();
    let timestamps = folded.iter().map(|entry| entry.ts);
    let (Some(first), Some(last)) = (timestamps.clone().min(), timestamps.max()) else {
        return Ok(());
    };
    let ids: Vec<Uuid> = folded.iter().map(|entry| entry.request_id).collect();
    sqlx::query(NETWORK_FOLD)
        .bind(first)
        .bind(last + TIMESTAMP_RESOLUTION)
        .bind(ids)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn touch_writer_seen(
    conn: &mut PgConnection,
    gap_features: &[&str],
    started: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO api_rollup_state (name, watermark) VALUES ($1, now())
         ON CONFLICT (name) DO UPDATE SET watermark = excluded.watermark",
    )
    .bind(WRITER_SEEN)
    .execute(&mut *conn)
    .await?;
    if gap_features.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE api_coverage_gap SET to_ts = now() WHERE feature = ANY($1) AND from_ts = $2",
    )
    .bind(gap_features)
    .bind(started)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn record_attribution_start(
    conn: &mut PgConnection,
    since: DateTime<Utc>,
    sources: SubjectSources,
) -> Result<(), sqlx::Error> {
    let features: &[&str] = match sources.enabled() {
        true => &[ATTRIBUTION_FEATURE, SUBJECTS_FEATURE],
        false => &[ATTRIBUTION_FEATURE],
    };
    sqlx::query(
        "INSERT INTO api_coverage (feature, since, note)
         SELECT feature, $2, 'recorded by the request log writer' FROM UNNEST($1::text[]) AS feature
         ON CONFLICT (feature) DO NOTHING",
    )
    .bind(features)
    .bind(since)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn is_committed(conn: &mut PgConnection, entry: &LogEntry) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM api_request_log WHERE ts = $1 AND request_id = $2)",
    )
    .bind(entry.ts)
    .bind(entry.request_id)
    .fetch_one(conn)
    .await
}

pub async fn record_startup_gaps(
    conn: &mut PgConnection,
    tolerance: chrono::Duration,
    sources: SubjectSources,
    started: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let writer_seen: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT watermark FROM api_rollup_state WHERE name = $1")
            .bind(WRITER_SEEN)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some(seen) = writer_seen {
        record_legacy_writer_gap(conn, seen, tolerance).await?;
    }
    let features = sources.gap_features();
    if features.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO api_coverage_gap (feature, from_ts, to_ts)
         SELECT feature, $2, $2 FROM UNNEST($1::text[]) AS feature
         ON CONFLICT DO NOTHING",
    )
    .bind(features)
    .bind(started)
    .execute(conn)
    .await?;
    Ok(())
}

async fn record_legacy_writer_gap(
    conn: &mut PgConnection,
    writer_seen: DateTime<Utc>,
    tolerance: chrono::Duration,
) -> Result<(), sqlx::Error> {
    let newest: Option<(DateTime<Utc>, Option<Uuid>)> =
        sqlx::query_as("SELECT ts, request_id FROM api_request_log ORDER BY ts DESC LIMIT 1")
            .fetch_optional(&mut *conn)
            .await?;
    let Some((ts, request_id)) = newest else {
        return Ok(());
    };
    let legacy = request_id.is_none_or(|id| id.get_version_num() != TIME_ORDERED_VERSION);
    if !legacy || ts <= writer_seen + tolerance {
        return Ok(());
    }
    tracing::warn!(%writer_seen, newest = %ts, "rows from an older writer found; recording a coverage gap");
    sqlx::query(
        "INSERT INTO api_coverage_gap (feature, from_ts, to_ts)
         SELECT feature, $2, now() FROM UNNEST($1::text[]) AS feature
         ON CONFLICT DO NOTHING",
    )
    .bind(&[ATTRIBUTION_FEATURE, SUBJECTS_FEATURE][..])
    .bind(writer_seen)
    .execute(conn)
    .await?;
    Ok(())
}

fn status_code(status: u16) -> i16 {
    status as i16
}
