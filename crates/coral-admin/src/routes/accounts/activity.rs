use std::collections::BTreeMap;

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use reqlog::Cause;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::profile::Totals;
use super::windows::{ActivityWindow, Span};
use super::{AccountTarget, FIRST_ERROR_STATUS};
use crate::error::{AdminError, Context};
use crate::serde_ts;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityQuery {
    window: ActivityWindow,
}

#[derive(Serialize)]
pub struct Activity {
    #[serde(serialize_with = "serde_ts::micros")]
    from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    to: DateTime<Utc>,
    step_s: i64,
    totals: Totals,
    mentions: i64,
    series: Vec<ActivityPoint>,
    breakdown: Vec<BreakdownRow>,
}

#[derive(Serialize)]
struct ActivityPoint {
    #[serde(serialize_with = "serde_ts::micros")]
    t: DateTime<Utc>,
    requests: i64,
    rate_limited_caller: i64,
    rate_limited_upstream: i64,
    errors: i64,
}

#[derive(Serialize)]
struct BreakdownRow {
    route: Option<String>,
    method: String,
    status: i16,
    cause: Cause,
    requests: i64,
}

#[derive(FromRow)]
struct BucketRow {
    t: DateTime<Utc>,
    requests: i64,
    caller: i64,
    upstream: i64,
    errors: i64,
    mentions: i64,
}

#[derive(FromRow)]
struct GroupRow {
    route: Option<String>,
    method: String,
    status: i16,
    cause: i16,
    requests: i64,
}

pub async fn activity(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    query: Result<Query<ActivityQuery>, QueryRejection>,
) -> Result<Json<Activity>, AdminError> {
    let Query(query) = query?;
    let target = AccountTarget::resolve(&state, &raw).await?;
    let span = query.window.span(Utc::now());
    let (buckets, groups) =
        tokio::try_join!(buckets(&state, target, span), groups(&state, target, span))?;
    Ok(Json(assemble(span, buckets, groups)))
}

async fn buckets(
    state: &AppState,
    target: AccountTarget,
    span: Span,
) -> Result<Vec<BucketRow>, AdminError> {
    sqlx::query_as(
        "SELECT date_bin($5::int8 * interval '1 second', hour, timestamptz 'epoch') AS t,
                sum(requests)::int8 AS requests,
                coalesce(sum(requests) FILTER (WHERE cause = $6), 0)::int8 AS caller,
                coalesce(sum(requests) FILTER (WHERE cause = $7), 0)::int8 AS upstream,
                coalesce(sum(requests) FILTER (WHERE status >= $8 AND cause NOT IN ($6, $7)), 0)::int8 AS errors,
                sum(subjects)::int8 AS mentions
         FROM api_caller_hour
         WHERE (member_id = $1 OR actor_id = $2) AND hour >= $3 AND hour < $4
         GROUP BY 1 ORDER BY 1",
    )
    .bind(target.member_id)
    .bind(target.discord_id)
    .bind(span.from)
    .bind(span.to)
    .bind(span.step.num_seconds())
    .bind(Cause::CallerLimit.code())
    .bind(Cause::UpstreamLimit.code())
    .bind(FIRST_ERROR_STATUS)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's hourly activity")
}

async fn groups(
    state: &AppState,
    target: AccountTarget,
    span: Span,
) -> Result<Vec<GroupRow>, AdminError> {
    sqlx::query_as(
        "SELECT route, method, status, cause, sum(requests)::int8 AS requests
         FROM api_caller_hour
         WHERE (member_id = $1 OR actor_id = $2) AND hour >= $3 AND hour < $4
         GROUP BY route, method, status, cause
         ORDER BY requests DESC, route, method, status, cause",
    )
    .bind(target.member_id)
    .bind(target.discord_id)
    .bind(span.from)
    .bind(span.to)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's route breakdown")
}

fn assemble(span: Span, buckets: Vec<BucketRow>, groups: Vec<GroupRow>) -> Activity {
    let mut totals = Totals::default();
    let mut mentions = 0;
    let mut filled: BTreeMap<DateTime<Utc>, ActivityPoint> =
        bucket_starts(span).map(|t| (t, empty_point(t))).collect();
    for bucket in buckets {
        totals.requests += bucket.requests;
        totals.rate_limited_caller += bucket.caller;
        totals.rate_limited_upstream += bucket.upstream;
        totals.errors += bucket.errors;
        mentions += bucket.mentions;
        filled.insert(
            bucket.t,
            ActivityPoint {
                t: bucket.t,
                requests: bucket.requests,
                rate_limited_caller: bucket.caller,
                rate_limited_upstream: bucket.upstream,
                errors: bucket.errors,
            },
        );
    }
    Activity {
        from: span.from,
        to: span.to,
        step_s: span.step.num_seconds(),
        totals,
        mentions,
        series: filled.into_values().collect(),
        breakdown: groups.into_iter().map(breakdown_row).collect(),
    }
}

fn bucket_starts(span: Span) -> impl Iterator<Item = DateTime<Utc>> {
    std::iter::successors(Some(span.from), move |t| Some(*t + span.step))
        .take_while(move |t| *t < span.to)
}

fn empty_point(t: DateTime<Utc>) -> ActivityPoint {
    ActivityPoint {
        t,
        requests: 0,
        rate_limited_caller: 0,
        rate_limited_upstream: 0,
        errors: 0,
    }
}

fn breakdown_row(group: GroupRow) -> BreakdownRow {
    BreakdownRow {
        cause: Cause::from_code(group.cause).unwrap_or_else(|| {
            tracing::warn!(code = group.cause, "caller rollup row has an unknown cause");
            Cause::Internal
        }),
        route: group.route,
        method: group.method,
        status: group.status,
        requests: group.requests,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn missing_hours_read_as_zero_and_totals_add_up() {
        let now = Utc.with_ymd_and_hms(2026, 10, 15, 4, 47, 11).unwrap();
        let span = ActivityWindow::Day.span(now);
        let busy = Utc.with_ymd_and_hms(2026, 10, 15, 3, 0, 0).unwrap();
        let activity = assemble(
            span,
            vec![BucketRow {
                t: busy,
                requests: 10,
                caller: 3,
                upstream: 1,
                errors: 2,
                mentions: 7,
            }],
            Vec::new(),
        );
        assert_eq!(activity.series.len(), 24);
        assert_eq!(activity.totals.requests, 10);
        assert_eq!(activity.mentions, 7);
        let filled = activity
            .series
            .iter()
            .find(|point| point.t == busy)
            .unwrap();
        assert_eq!(filled.rate_limited_caller, 3);
        assert!(
            activity
                .series
                .iter()
                .filter(|point| point.t != busy)
                .all(|point| point.requests == 0)
        );
    }
}
