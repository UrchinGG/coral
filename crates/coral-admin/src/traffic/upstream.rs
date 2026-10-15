use chrono::{DateTime, TimeDelta, Utc};
use clients::HypixelBudget;
use coral_redis::minute_history::{
    CHANGED_SNAPSHOTS, FIRST_SNAPSHOTS, HISTORY_TTL_SECS, HYPIXEL_ERRORS, HYPIXEL_HISTORY,
    HYPIXEL_REQUESTS, SNAPSHOT_HISTORY,
};
use coral_redis::{LIMIT_WINDOW_SECS, RateLimiter, RedisPool};
use reqlog::rollup::{Span, floor_to};
use serde::Serialize;

use super::history::UpstreamHistory;
use super::range::{RangeBounds, TrafficRange};
use crate::error::{AdminError, Context};
use crate::serde_ts;

pub const BACKGROUND_SHARE: f64 = 0.3;
pub const BACKGROUND_LIMITER: &str = "hypixel:background";
pub const HYPIXEL_COUNTERS: &[&str] = &[HYPIXEL_REQUESTS, HYPIXEL_ERRORS];
pub const SNAPSHOT_COUNTERS: &[&str] = &[FIRST_SNAPSHOTS, CHANGED_SNAPSHOTS];

const DAY: TimeDelta = TimeDelta::days(1);

#[derive(Debug, Clone, Serialize)]
pub struct Upstream {
    #[serde(flatten)]
    pub range: RangeBounds,
    pub step_s: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub history_from: DateTime<Utc>,
    pub series: Vec<UpstreamPoint>,
    pub snapshots: Vec<SnapshotPoint>,
    pub background: Background,
    pub capacity: Option<HypixelBudget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UpstreamPoint {
    #[serde(serialize_with = "serde_ts::micros")]
    pub t: DateTime<Utc>,
    pub requests: i64,
    pub errors: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SnapshotPoint {
    #[serde(serialize_with = "serde_ts::micros")]
    pub t: DateTime<Utc>,
    pub first: i64,
    pub delta: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Background {
    pub used: i64,
    pub limit: Option<i64>,
    pub window_s: i64,
}

pub fn history_from(now: DateTime<Utc>) -> DateTime<Utc> {
    floor_to(now, DAY) - TimeDelta::seconds(HISTORY_TTL_SECS) + DAY
}

pub async fn upstream(
    redis: &RedisPool,
    history: &UpstreamHistory,
    range: TrafficRange,
    now: DateTime<Utc>,
) -> Result<Upstream, AdminError> {
    let retained_from = history_from(now);
    let read_from = range.span.from.max(retained_from);
    let to = range.span.to;
    let (hypixel, snapshots, background, capacity) = tokio::try_join!(
        history.read(redis, HYPIXEL_HISTORY, HYPIXEL_COUNTERS, read_from, to, now),
        history.read(
            redis,
            SNAPSHOT_HISTORY,
            SNAPSHOT_COUNTERS,
            read_from,
            to,
            now
        ),
        background_used(redis),
        capacity(redis),
    )?;
    let buckets: Vec<Span> = range
        .buckets()
        .map(|from| Span {
            from,
            to: from + range.step.delta(),
        })
        .collect();
    Ok(Upstream {
        range: range.bounds(),
        step_s: range.step.seconds(),
        history_from: retained_from,
        series: buckets
            .iter()
            .map(|bucket| UpstreamPoint {
                t: bucket.from,
                requests: hypixel.total(HYPIXEL_REQUESTS, bucket.from, bucket.to),
                errors: hypixel.total(HYPIXEL_ERRORS, bucket.from, bucket.to),
            })
            .collect(),
        snapshots: buckets
            .iter()
            .map(|bucket| SnapshotPoint {
                t: bucket.from,
                first: snapshots.total(FIRST_SNAPSHOTS, bucket.from, bucket.to),
                delta: snapshots.total(CHANGED_SNAPSHOTS, bucket.from, bucket.to),
            })
            .collect(),
        background: Background {
            used: background,
            limit: capacity.map(background_limit),
            window_s: LIMIT_WINDOW_SECS,
        },
        capacity,
    })
}

pub async fn capacity(redis: &RedisPool) -> Result<Option<HypixelBudget>, AdminError> {
    clients::hypixel_budget(&mut redis.connection())
        .await
        .map_err(|error| AdminError::Redis(format!("read the upstream capacity: {error}")))
}

async fn background_used(redis: &RedisPool) -> Result<i64, AdminError> {
    let used = RateLimiter::new(redis.clone())
        .usage_many(&[BACKGROUND_LIMITER.to_string()])
        .await
        .context("count the background refresh budget")?;
    Ok(used.into_iter().sum())
}

fn background_limit(capacity: HypixelBudget) -> i64 {
    (capacity.limit as f64 * BACKGROUND_SHARE).round() as i64
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn history_reaches_back_to_the_oldest_day_that_cannot_have_expired() {
        assert_eq!(history_from(at(15, 6, 30)), at(1, 0, 0));
    }

    #[test]
    fn the_background_share_is_a_fraction_of_the_capacity() {
        let capacity = HypixelBudget {
            used: 0,
            limit: 300,
            at: at(15, 0, 0),
        };
        assert_eq!(background_limit(capacity), 90);
    }
}
