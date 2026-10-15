use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use coral_redis::RedisPool;
use coral_redis::minute_history::{DayCounts, MinuteHistory, days_touched};
use moka::future::Cache;

use crate::error::{AdminError, Context};

pub const DAY_CLOSE_DELAY: TimeDelta = TimeDelta::minutes(2);

const CLOSED_DAYS_KEPT: u64 = 64;

#[derive(Clone)]
pub struct UpstreamHistory {
    closed: Cache<ClosedDay, Arc<DayCounts>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClosedDay {
    key: String,
    counters: &'static [&'static str],
}

#[derive(Debug, Default)]
pub struct HistoryDays {
    days: Vec<Arc<DayCounts>>,
}

impl UpstreamHistory {
    pub fn new() -> Self {
        Self {
            closed: Cache::builder().max_capacity(CLOSED_DAYS_KEPT).build(),
        }
    }

    pub async fn read(
        &self,
        redis: &RedisPool,
        history: MinuteHistory,
        counters: &'static [&'static str],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<HistoryDays, AdminError> {
        let mut known = HashMap::new();
        let mut missing = Vec::new();
        for day in days_touched(from, to) {
            match self.closed.get(&closed_day(history, counters, day)).await {
                Some(counts) => {
                    known.insert(day, counts);
                }
                None => missing.push(day),
            }
        }
        let read = history
            .read_days(&mut redis.connection(), &missing, counters)
            .await
            .context("read the per-minute upstream history")?;
        for counts in read {
            let counts = Arc::new(counts);
            if is_closed(counts.day(), now) {
                let day = closed_day(history, counters, counts.day());
                self.closed.insert(day, counts.clone()).await;
            }
            known.insert(counts.day(), counts);
        }
        let mut days: Vec<Arc<DayCounts>> = known.into_values().collect();
        days.sort_by_key(|counts| counts.day());
        Ok(HistoryDays { days })
    }
}

impl HistoryDays {
    pub fn total(&self, counter: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> i64 {
        self.days
            .iter()
            .map(|counts| counts.total(counter, from, to))
            .sum()
    }
}

fn closed_day(history: MinuteHistory, counters: &'static [&'static str], day: i64) -> ClosedDay {
    ClosedDay {
        key: history.day_key(day),
        counters,
    }
}

fn is_closed(day: i64, now: DateTime<Utc>) -> bool {
    let next_day = DateTime::from_timestamp((day + 1) * TimeDelta::days(1).num_seconds(), 0);
    next_day.is_some_and(|next_day| next_day + DAY_CLOSE_DELAY <= now)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    const OCTOBER_14: i64 = 20_740;

    #[test]
    fn a_day_closes_a_little_after_midnight() {
        let midnight = Utc.with_ymd_and_hms(2026, 10, 15, 0, 0, 0).unwrap();
        assert!(!is_closed(OCTOBER_14, midnight + TimeDelta::minutes(1)));
        assert!(is_closed(OCTOBER_14, midnight + DAY_CLOSE_DELAY));
        assert!(!is_closed(OCTOBER_14 + 1, midnight + TimeDelta::hours(12)));
    }
}
