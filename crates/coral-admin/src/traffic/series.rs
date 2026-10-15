use std::collections::HashMap;

use chrono::{DateTime, Utc};
use reqlog::rollup::{CALLER_WATERMARK, TRAFFIC_WATERMARK};
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};

use super::class::OUTCOME_SUMS;
use super::latency::{BUCKET_SUMS, Histogram, P95};
use super::range::{RangeBounds, Step, TrafficRange};
use super::rows::{RowSpans, TrafficFilter};
use super::summary::Classes;
use crate::error::{AdminError, Context};
use crate::serde_ts;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SeriesFilter {
    pub traffic: TrafficFilter,
    pub member: Option<i64>,
    pub actor: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Series {
    pub step_s: i64,
    #[serde(flatten)]
    pub range: RangeBounds,
    pub points: Vec<SeriesPoint>,
    pub last_partial: bool,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub rolled_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeriesPoint {
    #[serde(serialize_with = "serde_ts::micros")]
    pub t: DateTime<Utc>,
    pub requests: i64,
    #[serde(flatten)]
    pub classes: Classes,
    pub p95: Option<i64>,
}

#[derive(Debug, FromRow)]
struct BucketRow {
    t: DateTime<Utc>,
    requests: i64,
    #[sqlx(flatten)]
    classes: Classes,
    latency_max_ms: Option<i32>,
    buckets: Option<Vec<i64>>,
}

impl SeriesFilter {
    fn by_caller(&self) -> bool {
        self.member.is_some() || self.actor.is_some()
    }
}

pub async fn series(
    pool: &PgPool,
    range: TrafficRange,
    filter: SeriesFilter,
) -> Result<Series, AdminError> {
    let (range, watermark_name) = match filter.by_caller() {
        true => (range.hourly(), CALLER_WATERMARK),
        false => (range, TRAFFIC_WATERMARK),
    };
    let rows = async {
        match filter.by_caller() {
            true => caller_buckets(pool, range, filter).await,
            false => traffic_buckets(pool, range, filter.traffic).await,
        }
    };
    let (rows, watermark) = tokio::try_join!(rows, watermark(pool, watermark_name))?;
    Ok(Series {
        step_s: range.step.seconds(),
        range: range.bounds(),
        points: filled(range, rows),
        last_partial: watermark.is_none_or(|watermark| range.span.to > watermark),
        rolled_to: watermark,
    })
}

pub async fn watermark(pool: &PgPool, name: &str) -> Result<Option<DateTime<Utc>>, AdminError> {
    sqlx::query_scalar("SELECT watermark FROM api_rollup_state WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await
        .context(format!("read the {name} rollup watermark"))
}

async fn traffic_buckets(
    pool: &PgPool,
    range: TrafficRange,
    filter: TrafficFilter,
) -> Result<Vec<BucketRow>, AdminError> {
    let mut query = QueryBuilder::new("SELECT date_bin(make_interval(secs => ");
    query.push_bind(range.step.seconds() as f64).push(format!(
        "), t, timestamptz 'epoch') AS t, sum(requests)::bigint AS requests, {}, \
         max(latency_max_ms) AS latency_max_ms, {} AS buckets FROM (",
        *OUTCOME_SUMS, *BUCKET_SUMS
    ));
    row_spans(range).push_rows(&mut query, &filter);
    query.push(") rows GROUP BY 1");
    query
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("bucket the traffic rollups")
}

async fn caller_buckets(
    pool: &PgPool,
    range: TrafficRange,
    filter: SeriesFilter,
) -> Result<Vec<BucketRow>, AdminError> {
    let mut query = QueryBuilder::new(format!(
        "SELECT hour AS t, sum(requests)::bigint AS requests, {}, \
         NULL::integer AS latency_max_ms, NULL::bigint[] AS buckets \
         FROM api_caller_hour WHERE hour >= ",
        *OUTCOME_SUMS
    ));
    query
        .push_bind(range.span.from)
        .push(" AND hour < ")
        .push_bind(range.span.to);
    push_caller_conditions(&mut query, filter);
    query.push(" GROUP BY 1");
    query
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("bucket the caller rollups")
}

fn push_caller_conditions(query: &mut QueryBuilder<'_, Postgres>, filter: SeriesFilter) {
    if let Some(member) = filter.member {
        query.push(" AND member_id = ").push_bind(member);
    }
    if let Some(actor) = filter.actor {
        query.push(" AND actor_id = ").push_bind(actor);
    }
    filter.traffic.push_conditions(query);
}

fn row_spans(range: TrafficRange) -> RowSpans {
    match range.step {
        Step::Hour => RowSpans::covering(range),
        Step::Minute | Step::FiveMinutes => RowSpans::minutes(range.span),
    }
}

fn filled(range: TrafficRange, rows: Vec<BucketRow>) -> Vec<SeriesPoint> {
    let mut by_bucket: HashMap<DateTime<Utc>, BucketRow> =
        rows.into_iter().map(|row| (row.t, row)).collect();
    range
        .buckets()
        .map(|t| match by_bucket.remove(&t) {
            Some(row) => point(row),
            None => SeriesPoint {
                t,
                requests: 0,
                classes: Classes::default(),
                p95: None,
            },
        })
        .collect()
}

fn point(row: BucketRow) -> SeriesPoint {
    let p95 = row.buckets.and_then(|buckets| {
        let mut histogram = Histogram::default();
        histogram.add(&buckets, row.latency_max_ms);
        histogram.percentile(P95)
    });
    SeriesPoint {
        t: row.t,
        requests: row.requests,
        classes: row.classes,
        p95,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone};
    use reqlog::rollup::Span;

    use super::*;
    use crate::traffic::range::Source;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, hour, minute, 0).unwrap()
    }

    fn five_minute_range() -> TrafficRange {
        TrafficRange {
            span: Span::new(at(10, 0), at(10, 20)).unwrap(),
            step: Step::FiveMinutes,
            source: Source::Minutes,
        }
    }

    #[test]
    fn every_bucket_has_a_point_and_missing_buckets_count_zero() {
        let row = BucketRow {
            t: at(10, 5),
            requests: 7,
            classes: Classes {
                c2xx: 7,
                ..Classes::default()
            },
            latency_max_ms: Some(20),
            buckets: Some(vec![0, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        };
        let points = filled(five_minute_range(), vec![row]);
        assert_eq!(points.len(), 4);
        assert_eq!(points[0].requests, 0);
        assert_eq!(points[0].p95, None);
        assert_eq!(points[1].t, at(10, 5));
        assert_eq!(points[1].requests, 7);
        assert!(points[1].p95.is_some());
        assert_eq!(points[3].t, at(10, 15));
    }

    #[test]
    fn caller_buckets_carry_no_latency() {
        let row = BucketRow {
            t: at(10, 0),
            requests: 3,
            classes: Classes::default(),
            latency_max_ms: None,
            buckets: None,
        };
        assert_eq!(point(row).p95, None);
    }

    #[test]
    fn hourly_steps_read_whole_hours_and_finer_steps_read_minutes() {
        let fine = five_minute_range();
        assert_eq!(row_spans(fine), RowSpans::minutes(fine.span));
        let hourly = TrafficRange {
            span: Span::new(at(6, 0), at(6, 0) + TimeDelta::days(3)).unwrap(),
            step: Step::Hour,
            source: Source::Hours,
        };
        assert_eq!(row_spans(hourly), RowSpans::covering(hourly));
    }

    #[test]
    fn points_serialize_their_classes_inline() {
        let value = serde_json::to_value(SeriesPoint {
            t: at(10, 0),
            requests: 2,
            classes: Classes {
                c429_caller: 2,
                ..Classes::default()
            },
            p95: Some(12),
        })
        .unwrap();
        assert_eq!(value["c429_caller"], 2);
        assert_eq!(value["t"], "2026-10-15T10:00:00.000000Z");
        assert_eq!(value["p95"], 12);
    }
}
