use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use redis::aio::ConnectionManager;

use crate::{COUNTER_DAY_FORMAT, COUNTER_TTL_SECS, DROPPED_COUNTER_KEY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    Overflow,
    Closed,
    RowError,
}

#[derive(Debug, Default)]
pub struct DropCounters {
    overflow: AtomicU64,
    closed: AtomicU64,
    row_error: AtomicU64,
}

impl DropReason {
    const ALL: [Self; 3] = [Self::Overflow, Self::Closed, Self::RowError];

    fn label(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
            Self::Closed => "closed",
            Self::RowError => "row_error",
        }
    }
}

impl DropCounters {
    pub fn count(&self, reason: DropReason, entries: u64) {
        self.counter(reason).fetch_add(entries, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn pending(&self, reason: DropReason) -> u64 {
        self.counter(reason).load(Ordering::Relaxed)
    }

    pub async fn report(&self, redis: &mut ConnectionManager) {
        let taken: Vec<(DropReason, u64)> = DropReason::ALL
            .into_iter()
            .map(|reason| (reason, self.counter(reason).swap(0, Ordering::Relaxed)))
            .filter(|(_, entries)| *entries > 0)
            .collect();
        if taken.is_empty() {
            return;
        }
        for (reason, entries) in &taken {
            tracing::warn!(
                reason = reason.label(),
                entries,
                "request log entries dropped"
            );
        }
        let fields: Vec<(String, u64)> = taken
            .iter()
            .map(|(reason, entries)| (daily_field(reason.label()), *entries))
            .collect();
        if let Err(e) = increment_daily(redis, DROPPED_COUNTER_KEY, &fields).await {
            tracing::warn!(error = %e, "request log drop counts not published; retrying");
            for (reason, entries) in taken {
                self.count(reason, entries);
            }
        }
    }

    fn counter(&self, reason: DropReason) -> &AtomicU64 {
        match reason {
            DropReason::Overflow => &self.overflow,
            DropReason::Closed => &self.closed,
            DropReason::RowError => &self.row_error,
        }
    }
}

pub fn daily_field(name: &str) -> String {
    format!("{}:{name}", Utc::now().format(COUNTER_DAY_FORMAT))
}

pub async fn increment_daily(
    redis: &mut ConnectionManager,
    key: &str,
    fields: &[(String, u64)],
) -> Result<(), redis::RedisError> {
    let mut pipe = redis::pipe();
    for (field, amount) in fields {
        pipe.hincr(key, field, *amount).ignore();
    }
    pipe.expire(key, COUNTER_TTL_SECS).ignore();
    pipe.query_async(redis).await
}
