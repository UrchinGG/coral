use std::collections::HashMap;

use chrono::{DateTime, TimeDelta, Utc};
use reqlog::rollup::{HOUR, floor_to};
use serde::Serialize;
use sqlx::{FromRow, PgPool, QueryBuilder};

use super::class::{errors_predicate, rate_limited_predicate, requests_where};
use super::latency::{BUCKET_SUMS, Histogram, Percentiles};
use super::range::TrafficRange;
use super::rows::{RowSpans, TrafficFilter};
use crate::error::{AdminError, Context};

pub const SPARK_POINTS: i64 = 24;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteTraffic {
    pub route: Option<String>,
    pub method: String,
    pub requests: i64,
    pub errors: i64,
    pub error_rate: f64,
    pub rate_limited: i64,
    #[serde(flatten)]
    pub latency: Percentiles,
    pub spark: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SparkPlan {
    step: TimeDelta,
    origin: DateTime<Utc>,
    whole_hours: bool,
}

#[derive(Debug, FromRow)]
struct RouteTotals {
    route: Option<String>,
    method: String,
    requests: i64,
    errors: i64,
    rate_limited: i64,
    latency_max_ms: Option<i32>,
    buckets: Vec<i64>,
}

#[derive(Debug, FromRow)]
struct SparkRow {
    route: Option<String>,
    method: String,
    t: DateTime<Utc>,
    requests: i64,
}

type RouteKey = (Option<String>, String);

pub async fn routes(pool: &PgPool, range: TrafficRange) -> Result<Vec<RouteTraffic>, AdminError> {
    let plan = SparkPlan::for_range(range);
    let (totals, sparks) =
        tokio::try_join!(route_totals(pool, range), spark_rows(pool, range, plan))?;
    Ok(combined(totals, sparks, plan, range))
}

impl SparkPlan {
    fn for_range(range: TrafficRange) -> Self {
        let factor = (range.bucket_count() + SPARK_POINTS - 1) / SPARK_POINTS;
        let step = range.step.delta() * factor.max(1) as i32;
        let whole_hours = step.num_seconds() % HOUR.num_seconds() == 0;
        let origin = match whole_hours {
            true => floor_to(range.span.from, HOUR),
            false => range.span.from,
        };
        Self {
            step,
            origin,
            whole_hours,
        }
    }

    fn bins(self, range: TrafficRange) -> impl Iterator<Item = DateTime<Utc>> {
        let step = self.step;
        let to = range.span.to;
        std::iter::successors(Some(self.origin), move |at| Some(*at + step))
            .take_while(move |at| *at < to)
    }

    fn rows(self, range: TrafficRange) -> RowSpans {
        match self.whole_hours {
            true => RowSpans::covering(range),
            false => RowSpans::minutes(range.span),
        }
    }
}

async fn route_totals(pool: &PgPool, range: TrafficRange) -> Result<Vec<RouteTotals>, AdminError> {
    let mut query = QueryBuilder::new(format!(
        "SELECT route, method, sum(requests)::bigint AS requests, {} AS errors, \
         {} AS rate_limited, max(latency_max_ms) AS latency_max_ms, {} AS buckets FROM (",
        requests_where(&errors_predicate()),
        requests_where(&rate_limited_predicate()),
        *BUCKET_SUMS
    ));
    RowSpans::covering(range).push_rows(&mut query, &TrafficFilter::default());
    query.push(") rows GROUP BY route, method");
    query
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("sum the traffic rollups by route")
}

async fn spark_rows(
    pool: &PgPool,
    range: TrafficRange,
    plan: SparkPlan,
) -> Result<Vec<SparkRow>, AdminError> {
    let mut query = QueryBuilder::new("SELECT route, method, date_bin(make_interval(secs => ");
    query
        .push_bind(plan.step.num_seconds() as f64)
        .push("), t, ")
        .push_bind(plan.origin)
        .push(") AS t, sum(requests)::bigint AS requests FROM (");
    plan.rows(range)
        .push_rows(&mut query, &TrafficFilter::default());
    query.push(") rows GROUP BY 1, 2, 3");
    query
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("bucket the traffic rollups by route")
}

fn combined(
    totals: Vec<RouteTotals>,
    sparks: Vec<SparkRow>,
    plan: SparkPlan,
    range: TrafficRange,
) -> Vec<RouteTraffic> {
    let mut by_route: HashMap<RouteKey, HashMap<DateTime<Utc>, i64>> = HashMap::new();
    for spark in sparks {
        by_route
            .entry((spark.route, spark.method))
            .or_default()
            .insert(spark.t, spark.requests);
    }
    let mut routes: Vec<RouteTraffic> = totals
        .into_iter()
        .map(|totals| {
            let points = by_route
                .remove(&(totals.route.clone(), totals.method.clone()))
                .unwrap_or_default();
            let spark = plan
                .bins(range)
                .map(|bin| points.get(&bin).copied().unwrap_or(0))
                .collect();
            route_traffic(totals, spark)
        })
        .collect();
    routes.sort_by(|a, b| b.requests.cmp(&a.requests).then(a.route.cmp(&b.route)));
    routes
}

fn route_traffic(totals: RouteTotals, spark: Vec<i64>) -> RouteTraffic {
    let mut histogram = Histogram::default();
    histogram.add(&totals.buckets, totals.latency_max_ms);
    RouteTraffic {
        error_rate: totals.errors as f64 / totals.requests.max(1) as f64,
        route: totals.route,
        method: totals.method,
        requests: totals.requests,
        errors: totals.errors,
        rate_limited: totals.rate_limited,
        latency: histogram.percentiles(),
        spark,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use reqlog::rollup::Span;

    use super::*;
    use crate::traffic::range::{Source, Step};

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, hour, minute, 0).unwrap()
    }

    fn range(from: DateTime<Utc>, length: TimeDelta, step: Step, source: Source) -> TrafficRange {
        TrafficRange {
            span: Span::new(from, from + length).unwrap(),
            step,
            source,
        }
    }

    #[test]
    fn a_day_sparks_by_the_hour_from_the_hour_table() {
        let day = range(
            at(6, 40) - TimeDelta::days(1),
            TimeDelta::days(1),
            Step::FiveMinutes,
            Source::Minutes,
        );
        let plan = SparkPlan::for_range(day);
        assert_eq!(plan.step, HOUR);
        assert!(plan.whole_hours);
        assert_eq!(plan.origin, at(6, 0) - TimeDelta::days(1));
        assert_eq!(plan.bins(day).count(), 25);
    }

    #[test]
    fn an_hour_sparks_by_three_minutes_from_the_minute_table() {
        let hour = range(
            at(11, 35),
            TimeDelta::hours(1),
            Step::Minute,
            Source::Minutes,
        );
        let plan = SparkPlan::for_range(hour);
        assert_eq!(plan.step, TimeDelta::minutes(3));
        assert!(!plan.whole_hours);
        assert_eq!(plan.origin, at(11, 35));
        assert_eq!(plan.bins(hour).count(), 20);
        assert_eq!(plan.rows(hour), RowSpans::minutes(hour.span));
    }

    #[test]
    fn routes_rank_by_requests_with_zero_filled_sparks() {
        let hour = range(
            at(10, 0),
            TimeDelta::hours(1),
            Step::Minute,
            Source::Minutes,
        );
        let plan = SparkPlan::for_range(hour);
        let totals = |route: &str, requests| RouteTotals {
            route: Some(route.into()),
            method: "GET".into(),
            requests,
            errors: requests / 2,
            rate_limited: 0,
            latency_max_ms: Some(10),
            buckets: vec![requests, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        };
        let sparks = vec![SparkRow {
            route: Some("/v3/players".into()),
            method: "GET".into(),
            t: at(10, 3),
            requests: 8,
        }];
        let routes = combined(
            vec![totals("/v3/tags", 2), totals("/v3/players", 8)],
            sparks,
            plan,
            hour,
        );
        assert_eq!(routes[0].route.as_deref(), Some("/v3/players"));
        assert_eq!(routes[0].spark[1], 8);
        assert_eq!(routes[0].spark.iter().sum::<i64>(), 8);
        assert_eq!(routes[0].error_rate, 0.5);
        assert_eq!(routes[1].spark, vec![0; 20]);
    }
}
