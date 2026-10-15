use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use coral_redis::RedisPool;
use database::Database;
use reqlog::rollup::{NETWORK_FEATURE, ROLLUPS_FEATURE};
use reqlog::{COUNTER_DAY_FORMAT, DROPPED_COUNTER_KEY, UNMAPPED_COUNTER_KEY};
use serde::Serialize;
use sqlx::FromRow;
use tokio::sync::watch;

use crate::error::{AdminError, Context};
use crate::serde_ts;

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
pub const BY_NAME_SHARE_KEY: &str = "admin:backfill:by_name_share";

const DETAIL: &str = "detail";
const BODY_SUBJECTS: &str = "body_subjects";
const ATTRIBUTION: &str = "attribution";
const SUBJECTS: &str = "subjects";
const LOG_TABLE: &str = "api_request_log";
const DETAIL_TABLE: &str = "api_request_detail";
const SUBJECT_TABLE: &str = "api_request_subject";
const WRITER_SEEN: &str = "writer_seen";

#[derive(Clone)]
pub struct CoverageCache {
    current: watch::Receiver<Arc<Coverage>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Coverage {
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub log_from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub detail_from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub detail_since: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub attribution_from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub subjects_from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub body_subjects_from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub rollups_from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub network_from: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub subject_partitions_from: Option<DateTime<Utc>>,
    pub gaps: Vec<CoverageGap>,
    pub backfill: Backfill,
    pub writer: WriterHealth,
}

#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct CoverageGap {
    pub feature: String,
    #[serde(rename = "from", serialize_with = "serde_ts::micros")]
    pub from_ts: DateTime<Utc>,
    #[serde(rename = "to", serialize_with = "serde_ts::micros")]
    pub to_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Backfill {
    pub by_name_share: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WriterHealth {
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub last_seen: Option<DateTime<Utc>>,
    pub lag_s: Option<f64>,
    pub dropped_today: BTreeMap<String, u64>,
    pub unmapped_today: BTreeMap<String, u64>,
}

#[derive(FromRow)]
struct Marker {
    feature: String,
    since: DateTime<Utc>,
}

#[derive(FromRow)]
struct PartitionFloor {
    parent: String,
    since: DateTime<Utc>,
}

struct Markers(HashMap<String, DateTime<Utc>>);

impl CoverageCache {
    pub async fn load(db: Arc<Database>, redis: RedisPool) -> Result<Self, AdminError> {
        let initial = read(&db, &redis).await?;
        let (sender, current) = watch::channel(Arc::new(initial));
        tokio::spawn(refresh_forever(db, redis, sender));
        Ok(Self { current })
    }

    #[cfg(test)]
    pub fn fixed(coverage: Coverage) -> Self {
        let (_, current) = watch::channel(Arc::new(coverage));
        Self { current }
    }

    pub fn current(&self) -> Coverage {
        let mut coverage = Coverage::clone(&self.current.borrow());
        coverage.writer.lag_s = coverage.writer.last_seen.map(seconds_since);
        coverage
    }
}

impl Coverage {
    pub fn searchable_subjects_from(&self) -> Option<DateTime<Utc>> {
        match (self.subjects_from, self.subject_partitions_from) {
            (Some(since), Some(floor)) => Some(since.max(floor)),
            (since, _) => since,
        }
    }
}

impl Markers {
    fn since(&self, feature: &str) -> Option<DateTime<Utc>> {
        self.0.get(feature).copied()
    }
}

async fn refresh_forever(
    db: Arc<Database>,
    redis: RedisPool,
    sender: watch::Sender<Arc<Coverage>>,
) {
    let mut ticks = tokio::time::interval(REFRESH_INTERVAL);
    ticks.tick().await;
    loop {
        ticks.tick().await;
        match read(&db, &redis).await {
            Ok(coverage) => {
                sender.send_replace(Arc::new(coverage));
            }
            Err(error) => {
                tracing::warn!(%error, "coverage refresh failed; serving the previous snapshot")
            }
        }
    }
}

async fn read(db: &Database, redis: &RedisPool) -> Result<Coverage, AdminError> {
    let (markers, floors, gaps, writer_seen, counters) = tokio::try_join!(
        markers(db),
        partition_floors(db),
        gaps(db),
        writer_seen(db),
        counters(redis),
    )?;
    let (dropped, unmapped, by_name_share) = counters;
    let today = Utc::now().date_naive();
    Ok(Coverage {
        log_from: floors.get(LOG_TABLE).copied(),
        detail_from: floors.get(DETAIL_TABLE).copied(),
        detail_since: markers.since(DETAIL),
        attribution_from: markers.since(ATTRIBUTION),
        subjects_from: markers.since(SUBJECTS),
        body_subjects_from: markers.since(BODY_SUBJECTS),
        rollups_from: markers.since(ROLLUPS_FEATURE),
        network_from: markers.since(NETWORK_FEATURE),
        subject_partitions_from: floors.get(SUBJECT_TABLE).copied(),
        gaps,
        backfill: Backfill {
            by_name_share: parse_shares(by_name_share)?,
        },
        writer: WriterHealth {
            last_seen: writer_seen,
            lag_s: None,
            dropped_today: counts_for_day(dropped, today),
            unmapped_today: counts_for_day(unmapped, today),
        },
    })
}

async fn markers(db: &Database) -> Result<Markers, AdminError> {
    let rows: Vec<Marker> = sqlx::query_as("SELECT feature, since FROM api_coverage")
        .fetch_all(db.pool())
        .await
        .context("read coverage markers")?;
    Ok(Markers(
        rows.into_iter()
            .map(|row| (row.feature, row.since))
            .collect(),
    ))
}

async fn partition_floors(db: &Database) -> Result<HashMap<String, DateTime<Utc>>, AdminError> {
    let rows: Vec<PartitionFloor> = sqlx::query_as(
        "SELECT parent.relname::text AS parent,
                min(to_date(right(child.relname, 8), 'YYYYMMDD'))::timestamp AT TIME ZONE 'UTC' AS since
         FROM pg_inherits i
         JOIN pg_class parent ON parent.oid = i.inhparent
         JOIN pg_class child ON child.oid = i.inhrelid
         WHERE parent.relname = ANY($1) AND child.relname ~ '_[0-9]{8}$'
         GROUP BY parent.relname",
    )
    .bind([LOG_TABLE, DETAIL_TABLE, SUBJECT_TABLE])
    .fetch_all(db.pool())
    .await
    .context("read request log partition bounds")?;
    Ok(rows
        .into_iter()
        .map(|row| (row.parent, row.since))
        .collect())
}

async fn gaps(db: &Database) -> Result<Vec<CoverageGap>, AdminError> {
    sqlx::query_as("SELECT feature, from_ts, to_ts FROM api_coverage_gap ORDER BY from_ts, feature")
        .fetch_all(db.pool())
        .await
        .context("read coverage gaps")
}

async fn writer_seen(db: &Database) -> Result<Option<DateTime<Utc>>, AdminError> {
    sqlx::query_scalar("SELECT watermark FROM api_rollup_state WHERE name = $1")
        .bind(WRITER_SEEN)
        .fetch_optional(db.pool())
        .await
        .context("read the request log writer heartbeat")
}

type CounterHashes = (
    HashMap<String, u64>,
    HashMap<String, u64>,
    HashMap<String, String>,
);

async fn counters(redis: &RedisPool) -> Result<CounterHashes, AdminError> {
    redis::pipe()
        .hgetall(DROPPED_COUNTER_KEY)
        .hgetall(UNMAPPED_COUNTER_KEY)
        .hgetall(BY_NAME_SHARE_KEY)
        .query_async(&mut redis.connection())
        .await
        .context("read request log counters")
}

fn counts_for_day(counts: HashMap<String, u64>, day: NaiveDate) -> BTreeMap<String, u64> {
    let prefix = format!("{}:", day.format(COUNTER_DAY_FORMAT));
    counts
        .into_iter()
        .filter_map(|(field, count)| Some((field.strip_prefix(&prefix)?.to_string(), count)))
        .collect()
}

fn parse_shares(raw: HashMap<String, String>) -> Result<BTreeMap<String, f64>, AdminError> {
    raw.into_iter()
        .map(|(day, share)| {
            let parsed = share.parse().map_err(|_| {
                AdminError::Redis(format!(
                    "{BY_NAME_SHARE_KEY} holds {share:?} for {day}, which is not a number"
                ))
            })?;
            Ok((day, parsed))
        })
        .collect()
}

fn seconds_since(at: DateTime<Utc>) -> f64 {
    (Utc::now() - at).as_seconds_f64()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 14, hour, 0, 0).unwrap()
    }

    #[test]
    fn only_todays_counters_are_reported_without_their_day_prefix() {
        let counts = HashMap::from([
            ("20261014:overflow".to_string(), 3),
            ("20261013:overflow".to_string(), 9),
            ("20261014:GET /v3/guild/sessions/daily".to_string(), 2),
        ]);
        let today = at(0).date_naive();
        assert_eq!(
            counts_for_day(counts, today),
            BTreeMap::from([
                ("GET /v3/guild/sessions/daily".to_string(), 2),
                ("overflow".to_string(), 3),
            ])
        );
    }

    #[test]
    fn malformed_shares_are_reported_not_dropped() {
        let raw = HashMap::from([("2026-10-01".to_string(), "lots".to_string())]);
        assert_eq!(parse_shares(raw).unwrap_err().code(), "redis");
        let raw = HashMap::from([("2026-10-01".to_string(), "0.26".to_string())]);
        assert_eq!(parse_shares(raw).unwrap()["2026-10-01"], 0.26);
    }

    #[test]
    fn subject_search_starts_at_the_later_of_marker_and_oldest_partition() {
        let coverage = Coverage {
            subjects_from: Some(at(21)),
            subject_partitions_from: Some(at(0)),
            ..Coverage::default()
        };
        assert_eq!(coverage.searchable_subjects_from(), Some(at(21)));
        let backfilled = Coverage {
            subjects_from: Some(at(0) - chrono::Duration::days(30)),
            subject_partitions_from: Some(at(0)),
            ..Coverage::default()
        };
        assert_eq!(backfilled.searchable_subjects_from(), Some(at(0)));
    }

    #[test]
    fn coverage_serializes_in_the_boot_shape() {
        let coverage = Coverage {
            attribution_from: Some(at(21)),
            gaps: vec![CoverageGap {
                feature: "subjects".into(),
                from_ts: at(1),
                to_ts: at(2),
            }],
            ..Coverage::default()
        };
        let value = serde_json::to_value(&coverage).unwrap();
        assert_eq!(value["attribution_from"], "2026-10-14T21:00:00.000000Z");
        assert_eq!(value["log_from"], serde_json::Value::Null);
        assert_eq!(
            value["gaps"],
            json!([{ "feature": "subjects", "from": "2026-10-14T01:00:00.000000Z", "to": "2026-10-14T02:00:00.000000Z" }])
        );
        assert!(value.get("subject_partitions_from").is_none());
        assert_eq!(
            value["writer"],
            json!({ "last_seen": null, "lag_s": null, "dropped_today": {}, "unmapped_today": {} })
        );
        assert_eq!(value["backfill"], json!({ "by_name_share": {} }));
    }

    #[test]
    fn current_reports_writer_lag_at_read_time() {
        let coverage = Coverage {
            writer: WriterHealth {
                last_seen: Some(Utc::now() - chrono::Duration::seconds(90)),
                ..WriterHealth::default()
            },
            ..Coverage::default()
        };
        let lag = CoverageCache::fixed(coverage)
            .current()
            .writer
            .lag_s
            .unwrap();
        assert!((90.0..95.0).contains(&lag), "{lag}");
    }

    #[tokio::test]
    async fn reads_markers_partitions_gaps_and_counters() {
        let (Ok(db_url), Ok(redis_url)) =
            (std::env::var("DATABASE_URL"), std::env::var("REDIS_URL"))
        else {
            return;
        };
        let db = Database::connect(&db_url).await.unwrap();
        let redis = RedisPool::connect(&redis_url).await.unwrap();
        reqlog::partitions::create_upcoming(db.pool())
            .await
            .unwrap();
        let today = Utc::now().date_naive();
        let field = format!("{}:row_error", today.format(COUNTER_DAY_FORMAT));
        let _: () = redis::cmd("HSET")
            .arg(DROPPED_COUNTER_KEY)
            .arg(&field)
            .arg(4)
            .query_async(&mut redis.connection())
            .await
            .unwrap();

        let coverage = read(&db, &redis).await.unwrap();

        assert_eq!(
            coverage.detail_since,
            Some(Utc.with_ymd_and_hms(2026, 10, 14, 3, 14, 32).unwrap())
        );
        assert_eq!(coverage.body_subjects_from, coverage.detail_since);
        let today_start = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
        assert!(coverage.log_from.is_some_and(|from| from <= today_start));
        assert!(coverage.subject_partitions_from.is_some());
        assert_eq!(coverage.writer.dropped_today.get("row_error"), Some(&4));
    }
}
