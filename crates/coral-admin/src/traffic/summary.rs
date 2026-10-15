use std::collections::HashMap;

use chrono::{DateTime, TimeDelta, Utc};
use reqlog::Cause;
use reqlog::rollup::TRAFFIC_WATERMARK;
use serde::Serialize;
use sqlx::{FromRow, PgPool, QueryBuilder};

use super::class::OUTCOME_SUMS;
use super::latency::{BUCKET_SUMS, Histogram};
use super::range::{RangeBounds, TrafficRange};
use super::rows::{RowSpans, TrafficFilter};
use crate::error::{AdminError, Context};
use crate::serde_ts;

pub const RPM_FRESHNESS: TimeDelta = TimeDelta::minutes(5);

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub range: RangeBounds,
    pub totals: Totals,
    pub causes: HashMap<Cause, i64>,
    pub prev: Option<Totals>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub requests: i64,
    pub errors: i64,
    pub rate_limited: RateLimited,
    pub classes: Classes,
    pub p50: Option<i64>,
    pub p95: Option<i64>,
    pub p99: Option<i64>,
    pub rpm_now: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RateLimited {
    pub caller: i64,
    pub upstream: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, FromRow)]
pub struct Classes {
    pub c2xx: i64,
    pub c4xx: i64,
    pub c429_caller: i64,
    pub c429_upstream: i64,
    pub c5xx: i64,
}

#[derive(Debug, FromRow)]
struct CauseRow {
    cause: i16,
    requests: i64,
    #[sqlx(flatten)]
    classes: Classes,
    latency_max_ms: Option<i32>,
    buckets: Vec<i64>,
}

#[derive(Debug, FromRow)]
struct LatestMinute {
    minute: DateTime<Utc>,
    requests: i64,
}

#[derive(Debug, Default)]
struct Folded {
    totals: Totals,
    causes: HashMap<Cause, i64>,
}

impl Classes {
    fn add(&mut self, other: Classes) {
        self.c2xx += other.c2xx;
        self.c4xx += other.c4xx;
        self.c429_caller += other.c429_caller;
        self.c429_upstream += other.c429_upstream;
        self.c5xx += other.c5xx;
    }
}

pub async fn summary(
    pool: &PgPool,
    range: TrafficRange,
    compare: bool,
    now: DateTime<Utc>,
) -> Result<Summary, AdminError> {
    let previous = async {
        match compare {
            true => totals(pool, range.previous(now)).await.map(Some),
            false => Ok(None),
        }
    };
    let (current, prev, latest) =
        tokio::try_join!(totals(pool, range), previous, latest_minute(pool),)?;
    let mut current = current;
    current.totals.rpm_now = latest
        .filter(|latest| latest.minute >= now - RPM_FRESHNESS)
        .map(|latest| latest.requests);
    Ok(Summary {
        range: range.bounds(),
        totals: current.totals,
        causes: current.causes,
        prev: prev.map(|prev| prev.totals),
        generated_at: now,
    })
}

async fn totals(pool: &PgPool, range: TrafficRange) -> Result<Folded, AdminError> {
    let mut query = QueryBuilder::new(format!(
        "SELECT cause, sum(requests)::bigint AS requests, {}, \
         max(latency_max_ms) AS latency_max_ms, {} AS buckets FROM (",
        *OUTCOME_SUMS, *BUCKET_SUMS
    ));
    RowSpans::covering(range).push_rows(&mut query, &TrafficFilter::default());
    query.push(") rows GROUP BY cause");
    let rows: Vec<CauseRow> = query
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("sum the traffic rollups")?;
    Ok(fold(rows))
}

async fn latest_minute(pool: &PgPool) -> Result<Option<LatestMinute>, AdminError> {
    sqlx::query_as(
        "SELECT state.watermark - interval '1 minute' AS minute,
                (SELECT coalesce(sum(requests), 0)::bigint FROM api_traffic_minute
                 WHERE minute = state.watermark - interval '1 minute') AS requests
         FROM api_rollup_state state WHERE state.name = $1",
    )
    .bind(TRAFFIC_WATERMARK)
    .fetch_optional(pool)
    .await
    .context("read the newest rolled-up minute")
}

fn fold(rows: Vec<CauseRow>) -> Folded {
    let mut folded = Folded::default();
    let mut histogram = Histogram::default();
    for row in rows {
        histogram.add(&row.buckets, row.latency_max_ms);
        folded.totals.requests += row.requests;
        folded.totals.classes.add(row.classes);
        *folded.causes.entry(cause_of(row.cause)).or_default() += row.requests;
    }
    let classes = folded.totals.classes;
    folded.totals.errors = classes.c4xx + classes.c5xx;
    folded.totals.rate_limited = RateLimited {
        caller: classes.c429_caller,
        upstream: classes.c429_upstream,
    };
    let percentiles = histogram.percentiles();
    folded.totals.p50 = percentiles.p50;
    folded.totals.p95 = percentiles.p95;
    folded.totals.p99 = percentiles.p99;
    folded
}

fn cause_of(code: i16) -> Cause {
    Cause::from_code(code).unwrap_or_else(|| {
        tracing::warn!(code, "a traffic rollup row has an unknown cause");
        Cause::Internal
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cause: Cause, classes: Classes, buckets: [i64; 12], max: i32) -> CauseRow {
        CauseRow {
            cause: cause.code(),
            requests: classes.c2xx
                + classes.c4xx
                + classes.c429_caller
                + classes.c429_upstream
                + classes.c5xx,
            classes,
            latency_max_ms: Some(max),
            buckets: buckets.to_vec(),
        }
    }

    #[test]
    fn folding_adds_classes_causes_and_one_shared_histogram() {
        let ok = row(
            Cause::Ok,
            Classes {
                c2xx: 90,
                ..Classes::default()
            },
            [0, 90, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            20,
        );
        let limited = row(
            Cause::UpstreamLimit,
            Classes {
                c429_upstream: 6,
                ..Classes::default()
            },
            [6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            4,
        );
        let failed = row(
            Cause::Internal,
            Classes {
                c5xx: 4,
                ..Classes::default()
            },
            [0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0],
            4_000,
        );
        let folded = fold(vec![ok, limited, failed]);
        assert_eq!(folded.totals.requests, 100);
        assert_eq!(folded.totals.errors, 4);
        assert_eq!(
            folded.totals.rate_limited,
            RateLimited {
                caller: 0,
                upstream: 6
            }
        );
        assert_eq!(folded.causes[&Cause::Ok], 90);
        assert_eq!(folded.causes[&Cause::UpstreamLimit], 6);
        assert!(folded.totals.p50.unwrap() < 25);
        assert!(folded.totals.p99.unwrap() >= 2_500);
    }

    #[test]
    fn an_empty_range_has_counts_of_zero_and_no_percentiles() {
        let folded = fold(Vec::new());
        assert_eq!(folded.totals, Totals::default());
        assert!(folded.causes.is_empty());
    }

    #[test]
    fn causes_serialize_by_their_wire_names() {
        let causes = HashMap::from([(Cause::CallerLimit, 3_i64)]);
        assert_eq!(
            serde_json::to_value(causes).unwrap(),
            serde_json::json!({ "caller_limit": 3 })
        );
    }
}
