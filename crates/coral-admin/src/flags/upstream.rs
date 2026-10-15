use chrono::{DateTime, TimeDelta, Utc};
use clients::{FlagLevel, HypixelBudget};
use coral_redis::RedisPool;
use coral_redis::minute_history::{HYPIXEL_ERRORS, HYPIXEL_HISTORY, HYPIXEL_REQUESTS};
use reqlog::rollup::Span;
use sqlx::PgPool;

use super::{Flag, FlagKind, day_key, grouped, percent};
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::traffic::caller::{CALLER_IDENTITY_COLUMNS, CALLER_IDENTITY_JOINS, CallerKey};
use crate::traffic::class::{Outcome, requests_where};
use crate::traffic::history::UpstreamHistory;
use crate::traffic::upstream::{HYPIXEL_COUNTERS, capacity};

pub const UPSTREAM_ERROR_RATE: f64 = 0.05;
pub const UPSTREAM_WINDOW: TimeDelta = TimeDelta::minutes(10);
pub const CAPACITY_ALERT: f64 = 0.9;
pub const TOP_SUBJECT_CALLERS: i64 = 5;

const FULL: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UpstreamCalls {
    requests: i64,
    errors: i64,
}

impl UpstreamCalls {
    fn failing_rate(self) -> Option<f64> {
        let rate = self.errors as f64 / self.requests as f64;
        (self.requests > 0 && rate > UPSTREAM_ERROR_RATE).then_some(rate)
    }
}

pub async fn flags(
    pool: &PgPool,
    redis: &RedisPool,
    history: &UpstreamHistory,
    now: DateTime<Utc>,
) -> Result<Vec<Flag>, AdminError> {
    let window = Span {
        from: now - UPSTREAM_WINDOW,
        to: now,
    };
    let (calls, capacity) =
        tokio::try_join!(upstream_calls(redis, history, window, now), capacity(redis))?;
    let capacity_flag = capacity.and_then(|capacity| capacity_flag(capacity, now));
    let Some(rate) = calls.failing_rate() else {
        return Ok(capacity_flag.into_iter().collect());
    };
    let (limited, callers) = tokio::try_join!(
        upstream_limited(pool, window.from),
        top_subject_callers(pool, window.from),
    )?;
    let errors = errors_flag(calls, rate, limited, now).with_refs(callers);
    Ok(std::iter::once(errors).chain(capacity_flag).collect())
}

async fn upstream_calls(
    redis: &RedisPool,
    history: &UpstreamHistory,
    window: Span,
    now: DateTime<Utc>,
) -> Result<UpstreamCalls, AdminError> {
    let counts = history
        .read(
            redis,
            HYPIXEL_HISTORY,
            HYPIXEL_COUNTERS,
            window.from,
            window.to,
            now,
        )
        .await?;
    Ok(UpstreamCalls {
        requests: counts.total(HYPIXEL_REQUESTS, window.from, window.to),
        errors: counts.total(HYPIXEL_ERRORS, window.from, window.to),
    })
}

async fn upstream_limited(pool: &PgPool, since: DateTime<Utc>) -> Result<i64, AdminError> {
    sqlx::query_scalar(&format!(
        "SELECT {} FROM api_traffic_minute WHERE minute >= $1",
        requests_where(&Outcome::UpstreamLimited.predicate())
    ))
    .bind(since)
    .fetch_one(pool)
    .await
    .context("count requests refused for upstream limits")
}

async fn top_subject_callers(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<EntityRef>, AdminError> {
    let keys: Vec<CallerKey> = sqlx::query_as(&format!(
        "WITH grouped AS (
             SELECT key_kind, member_id, actor_id, key_prefix, service, count(*) AS subjects
             FROM api_request_subject WHERE ts >= $1
             GROUP BY key_kind, member_id, actor_id, key_prefix, service
             ORDER BY subjects DESC LIMIT $2
         )
         SELECT {CALLER_IDENTITY_COLUMNS} FROM grouped {CALLER_IDENTITY_JOINS}
         ORDER BY grouped.subjects DESC"
    ))
    .bind(since)
    .bind(TOP_SUBJECT_CALLERS)
    .fetch_all(pool)
    .await
    .context("rank callers by player lookups")?;
    Ok(keys.iter().map(CallerKey::requester).collect())
}

fn errors_flag(calls: UpstreamCalls, rate: f64, limited: i64, now: DateTime<Utc>) -> Flag {
    Flag::new(
        FlagKind::Upstream,
        FlagLevel::Danger,
        format!("upstream:errors:{}", day_key(now)),
        format!(
            "Upstream errors at {} over the last 10 min ({} of {} calls); {} requests answered 429 for upstream limits",
            percent(rate),
            grouped(calls.errors),
            grouped(calls.requests),
            grouped(limited),
        ),
    )
}

fn capacity_flag(capacity: HypixelBudget, now: DateTime<Utc>) -> Option<Flag> {
    if capacity.limit <= 0 {
        return None;
    }
    let ratio = capacity.used as f64 / capacity.limit as f64;
    let level = match ratio {
        ratio if ratio >= FULL => FlagLevel::Danger,
        ratio if ratio >= CAPACITY_ALERT => FlagLevel::Warning,
        _ => return None,
    };
    Some(Flag::new(
        FlagKind::Upstream,
        level,
        format!("upstream:capacity:{}", day_key(now)),
        format!(
            "Upstream capacity at {} ({} of {} used)",
            percent(ratio),
            grouped(capacity.used),
            grouped(capacity.limit),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap()
    }

    fn budget(used: i64, limit: i64) -> HypixelBudget {
        HypixelBudget {
            used,
            limit,
            at: now(),
        }
    }

    #[test]
    fn errors_fire_only_above_five_percent_of_some_calls() {
        let calls = |requests, errors| UpstreamCalls { requests, errors };
        assert_eq!(calls(0, 0).failing_rate(), None);
        assert_eq!(calls(100, 5).failing_rate(), None);
        assert_eq!(calls(100, 6).failing_rate(), Some(0.06));
    }

    #[test]
    fn capacity_warns_at_ninety_percent_and_is_dangerous_when_full() {
        assert!(capacity_flag(budget(269, 300), now()).is_none());
        assert_eq!(
            capacity_flag(budget(270, 300), now()).unwrap().level,
            FlagLevel::Warning
        );
        assert_eq!(
            capacity_flag(budget(300, 300), now()).unwrap().level,
            FlagLevel::Danger
        );
        assert!(capacity_flag(budget(0, 0), now()).is_none());
    }

    #[test]
    fn the_error_flag_states_the_rate_and_the_refused_requests() {
        let flag = errors_flag(
            UpstreamCalls {
                requests: 2_000,
                errors: 140,
            },
            0.07,
            37,
            now(),
        );
        assert_eq!(flag.key, "upstream:errors:2026-10-15");
        assert_eq!(
            flag.summary,
            "Upstream errors at 7% over the last 10 min (140 of 2,000 calls); 37 requests answered 429 for upstream limits"
        );
    }
}
