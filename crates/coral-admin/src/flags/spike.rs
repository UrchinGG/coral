use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use clients::FlagLevel;
use moka::future::Cache;
use reqlog::rollup::{HOUR, REROLL_WINDOW, Span, floor_to};
use sqlx::{FromRow, PgPool};

use super::{Flag, FlagKind, day_key, grouped};
use crate::coverage::Coverage;
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

pub const SPIKE_MULTIPLIER: f64 = 3.0;
pub const SPIKE_BASELINE_FLOOR: f64 = 20.0;
pub const RECENT_SPAN: TimeDelta = TimeDelta::hours(24);
pub const BASELINE_SPAN: TimeDelta = TimeDelta::days(7);
pub const MIN_BASELINE: TimeDelta = TimeDelta::days(1);

const SETTLED_WINDOWS_KEPT: u64 = 4;

#[derive(Clone)]
pub struct SpikeBaselines {
    settled: Cache<Windows, Arc<Vec<SettledMember>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Windows {
    baseline: Span,
    settled_to: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
struct SettledMember {
    member_id: i64,
    discord_id: i64,
    baseline: i64,
    recent: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
struct LiveMember {
    member_id: i64,
    requests: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spike {
    member_id: i64,
    discord_id: i64,
    recent: i64,
    baseline: i64,
}

impl SpikeBaselines {
    pub fn new() -> Self {
        Self {
            settled: Cache::builder().max_capacity(SETTLED_WINDOWS_KEPT).build(),
        }
    }

    async fn settled(
        &self,
        pool: &PgPool,
        windows: Windows,
    ) -> Result<Arc<Vec<SettledMember>>, AdminError> {
        self.settled
            .try_get_with(windows, async move {
                settled_members(pool, windows).await.map(Arc::new)
            })
            .await
            .map_err(Arc::unwrap_or_clone)
    }
}

impl Windows {
    fn baseline_days(self) -> f64 {
        (self.baseline.to - self.baseline.from).as_seconds_f64()
            / TimeDelta::days(1).as_seconds_f64()
    }

    fn baseline_floor(self) -> f64 {
        SPIKE_BASELINE_FLOOR * self.baseline_days()
    }
}

pub async fn flags(
    pool: &PgPool,
    baselines: &SpikeBaselines,
    coverage: &Coverage,
    now: DateTime<Utc>,
) -> Result<Vec<Flag>, AdminError> {
    let Some(windows) = windows(coverage.rollups_from, now) else {
        return Ok(Vec::new());
    };
    let (settled, live) = tokio::try_join!(
        baselines.settled(pool, windows),
        live_members(pool, windows.settled_to),
    )?;
    Ok(spikes(&settled, &live, windows)
        .into_iter()
        .map(|spike| spike_flag(spike, windows, now))
        .collect())
}

fn windows(rollups_from: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<Windows> {
    let recent_from = floor_to(now, HOUR) + HOUR - RECENT_SPAN;
    let baseline_from = (recent_from - BASELINE_SPAN).max(rollups_from?);
    let baseline = Span::new(baseline_from, recent_from)?;
    (baseline.to - baseline.from >= MIN_BASELINE).then(|| Windows {
        baseline,
        settled_to: floor_to(now - REROLL_WINDOW, HOUR),
    })
}

async fn settled_members(
    pool: &PgPool,
    windows: Windows,
) -> Result<Vec<SettledMember>, AdminError> {
    sqlx::query_as(
        "WITH per_member AS (
             SELECT member_id,
                    coalesce(sum(requests) FILTER (WHERE hour < $2), 0)::bigint AS baseline,
                    coalesce(sum(requests) FILTER (WHERE hour >= $2), 0)::bigint AS recent
             FROM api_caller_hour
             WHERE hour >= $1 AND hour < $3 AND member_id IS NOT NULL
             GROUP BY member_id
         )
         SELECT per_member.member_id, m.discord_id, per_member.baseline, per_member.recent
         FROM per_member JOIN members m ON m.id = per_member.member_id
         WHERE per_member.baseline >= $4",
    )
    .bind(windows.baseline.from)
    .bind(windows.baseline.to)
    .bind(windows.settled_to)
    .bind(windows.baseline_floor())
    .fetch_all(pool)
    .await
    .context("sum the settled caller hours of every member")
}

async fn live_members(
    pool: &PgPool,
    settled_to: DateTime<Utc>,
) -> Result<HashMap<i64, i64>, AdminError> {
    let rows: Vec<LiveMember> = sqlx::query_as(
        "SELECT member_id, sum(requests)::bigint AS requests FROM api_caller_hour
         WHERE hour >= $1 AND member_id IS NOT NULL GROUP BY member_id",
    )
    .bind(settled_to)
    .fetch_all(pool)
    .await
    .context("sum the caller hours that can still change")?;
    Ok(rows
        .into_iter()
        .map(|row| (row.member_id, row.requests))
        .collect())
}

fn spikes(settled: &[SettledMember], live: &HashMap<i64, i64>, windows: Windows) -> Vec<Spike> {
    let days = windows.baseline_days();
    settled
        .iter()
        .map(|member| Spike {
            member_id: member.member_id,
            discord_id: member.discord_id,
            recent: member.recent + live.get(&member.member_id).copied().unwrap_or(0),
            baseline: member.baseline,
        })
        .filter(|spike| {
            spike.baseline as f64 >= windows.baseline_floor()
                && spike.recent as f64 >= SPIKE_MULTIPLIER * spike.baseline as f64 / days
        })
        .collect()
}

fn spike_flag(spike: Spike, windows: Windows, now: DateTime<Utc>) -> Flag {
    let daily = spike.baseline as f64 / windows.baseline_days();
    let ratio = spike.recent as f64 / daily;
    Flag::new(
        FlagKind::Spike,
        FlagLevel::Warning,
        format!("spike:{}:{}", spike.discord_id, day_key(now)),
        format!(
            "{} requests in the last 24 h against a baseline of {} a day ({ratio:.1}×)",
            grouped(spike.recent),
            grouped(daily.round() as i64),
        ),
    )
    .with_refs(vec![EntityRef::Account {
        discord_id: spike.discord_id,
        member_id: Some(spike.member_id),
    }])
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    const SPIKING: i64 = 1_190_000_000_000_000_101;
    const STEADY: i64 = 1_190_000_000_000_000_102;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    fn member(member_id: i64, baseline: i64, recent: i64) -> SettledMember {
        SettledMember {
            member_id,
            discord_id: member_id + 1_000,
            baseline,
            recent,
        }
    }

    #[test]
    fn the_recent_day_includes_the_open_hour_and_the_baseline_is_the_week_before() {
        let windows = windows(Some(at(1, 0, 0)), at(15, 12, 30)).unwrap();
        assert_eq!(windows.baseline.from, at(7, 13, 0));
        assert_eq!(windows.baseline.to, at(14, 13, 0));
        assert_eq!(windows.baseline_days(), 7.0);
        assert_eq!(windows.settled_to, at(15, 12, 0));
    }

    #[test]
    fn the_previous_hour_stays_live_until_its_rows_can_no_longer_change() {
        let windows = windows(Some(at(1, 0, 0)), at(15, 12, 5)).unwrap();
        assert_eq!(windows.settled_to, at(15, 11, 0));
    }

    #[test]
    fn the_baseline_starts_no_earlier_than_the_rollups() {
        let windows = windows(Some(at(12, 1, 0)), at(15, 12, 30)).unwrap();
        assert_eq!(windows.baseline.from, at(12, 1, 0));
        assert_eq!(windows.baseline_days(), 2.5);
    }

    #[test]
    fn without_a_day_of_rolled_up_baseline_nothing_is_compared() {
        assert_eq!(windows(None, at(15, 12, 30)), None);
        assert_eq!(windows(Some(at(14, 0, 0)), at(15, 12, 30)), None);
    }

    #[test]
    fn live_hours_count_towards_the_recent_day() {
        let windows = windows(Some(at(1, 0, 0)), at(15, 12, 30)).unwrap();
        let settled = [
            member(1, 7_000, 2_000),
            member(2, 7_000, 2_999),
            member(3, 100, 900),
        ];
        let live = HashMap::from([(1, 1_000), (2, 0)]);
        let found = spikes(&settled, &live, windows);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].member_id, 1);
        assert_eq!(found[0].recent, 3_000);
    }

    #[test]
    fn a_spike_names_the_account_and_its_ratio() {
        let windows = windows(Some(at(1, 0, 0)), at(15, 12, 30)).unwrap();
        let flag = spike_flag(
            Spike {
                member_id: 21111,
                discord_id: 42,
                recent: 6_000,
                baseline: 7_000,
            },
            windows,
            at(15, 12, 30),
        );
        assert_eq!(flag.key, "spike:42:2026-10-15");
        assert_eq!(
            flag.summary,
            "6,000 requests in the last 24 h against a baseline of 1,000 a day (6.0×)"
        );
        assert_eq!(flag.refs.len(), 1);
    }

    async fn insert_member(pool: &PgPool, discord_id: i64) -> i64 {
        sqlx::query_scalar("INSERT INTO members (discord_id) VALUES ($1) RETURNING id")
            .bind(discord_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn insert_hour(pool: &PgPool, member_id: i64, hour: DateTime<Utc>, requests: i32) {
        sqlx::query(
            "INSERT INTO api_caller_hour (hour, key_kind, member_id, service, method, status, cause,
               requests, latency_sum_ms, subjects)
             VALUES ($1, 1, $2, 0, 'GET', 200, 0, $3, 0, 0)",
        )
        .bind(floor_to(hour, HOUR))
        .bind(member_id)
        .bind(requests)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn clear(pool: &PgPool) {
        sqlx::query("DELETE FROM api_caller_hour WHERE hour >= $1 AND hour < $2")
            .bind(Utc.with_ymd_and_hms(2019, 4, 1, 0, 0, 0).unwrap())
            .bind(Utc.with_ymd_and_hms(2019, 5, 1, 0, 0, 0).unwrap())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM members WHERE discord_id = ANY($1)")
            .bind([SPIKING, STEADY])
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn members_far_above_their_daily_baseline_are_flagged() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url).await.unwrap();
        clear(&pool).await;
        let now = Utc.with_ymd_and_hms(2019, 4, 20, 12, 30, 0).unwrap();
        let spiking = insert_member(&pool, SPIKING).await;
        let steady = insert_member(&pool, STEADY).await;
        for day in 2..=8 {
            let hour = now - TimeDelta::days(day);
            insert_hour(&pool, spiking, hour, 30).await;
            insert_hour(&pool, steady, hour, 30).await;
        }
        insert_hour(&pool, spiking, now - TimeDelta::hours(3), 60).await;
        insert_hour(&pool, spiking, now, 80).await;
        insert_hour(&pool, steady, now - TimeDelta::hours(3), 60).await;
        let coverage = Coverage {
            rollups_from: Some(now - TimeDelta::days(30)),
            ..Coverage::default()
        };
        let baselines = SpikeBaselines::new();
        let found = flags(&pool, &baselines, &coverage, now).await;
        clear(&pool).await;
        let keys: Vec<String> = found.unwrap().iter().map(|flag| flag.key.clone()).collect();
        assert_eq!(keys, vec![format!("spike:{SPIKING}:2019-04-20")]);
    }
}
