use std::collections::HashMap;
use std::ops::Range;

use chrono::{DateTime, Utc};
use redis::Pipeline;
use redis::aio::ConnectionManager;

pub const HYPIXEL_HISTORY: MinuteHistory = MinuteHistory { prefix: "hp:hist:" };
pub const HYPIXEL_REQUESTS: &str = "t";
pub const HYPIXEL_ERRORS: &str = "e";

pub const SNAPSHOT_HISTORY: MinuteHistory = MinuteHistory {
    prefix: "snap:hist:",
};
pub const FIRST_SNAPSHOTS: &str = "f";
pub const CHANGED_SNAPSHOTS: &str = "d";

pub const HISTORY_TTL_SECS: i64 = 15 * SECS_PER_DAY;

const SECS_PER_DAY: i64 = 86_400;
const SECS_PER_MINUTE: i64 = 60;
const MINUTES_PER_DAY: usize = 1_440;
const MICROS_PER_MINUTE: i64 = 60_000_000;
const MICROS_PER_DAY: i64 = SECS_PER_DAY * 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinuteHistory {
    prefix: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinuteSlot {
    pub key: String,
    pub minute: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayCounts {
    day: i64,
    by_counter: HashMap<String, Vec<i64>>,
}

impl MinuteHistory {
    pub fn day_key(self, day: i64) -> String {
        format!("{}{day}", self.prefix)
    }

    pub fn slot(self, at: DateTime<Utc>) -> MinuteSlot {
        let secs = at.timestamp();
        MinuteSlot {
            key: self.day_key(secs / SECS_PER_DAY),
            minute: (secs % SECS_PER_DAY) / SECS_PER_MINUTE,
        }
    }

    pub fn queue(self, pipe: &mut Pipeline, at: DateTime<Utc>, counters: &[&str]) {
        let slot = self.slot(at);
        for counter in counters {
            pipe.hincr(&slot.key, slot.field(counter), 1).ignore();
        }
        pipe.expire(&slot.key, HISTORY_TTL_SECS).ignore();
    }

    pub async fn record(
        self,
        conn: &mut ConnectionManager,
        at: DateTime<Utc>,
        counters: &[&str],
    ) -> Result<(), redis::RedisError> {
        let mut pipe = redis::pipe();
        self.queue(&mut pipe, at, counters);
        pipe.query_async(conn).await
    }

    pub async fn read_days(
        self,
        conn: &mut ConnectionManager,
        days: &[i64],
        counters: &[&str],
    ) -> Result<Vec<DayCounts>, redis::RedisError> {
        if days.is_empty() {
            return Ok(Vec::new());
        }
        let fields: Vec<String> = counters
            .iter()
            .flat_map(|counter| {
                (0..MINUTES_PER_DAY).map(move |minute| format!("{counter}{minute}"))
            })
            .collect();
        let mut pipe = redis::pipe();
        for day in days {
            pipe.cmd("HMGET").arg(self.day_key(*day)).arg(&fields);
        }
        let replies: Vec<Vec<Option<i64>>> = pipe.query_async(conn).await?;
        Ok(days
            .iter()
            .zip(replies)
            .map(|(day, values)| DayCounts::new(*day, counters, &values))
            .collect())
    }
}

impl MinuteSlot {
    pub fn field(&self, counter: &str) -> String {
        format!("{counter}{}", self.minute)
    }
}

impl DayCounts {
    pub fn new(day: i64, counters: &[&str], values: &[Option<i64>]) -> Self {
        let by_counter = counters
            .iter()
            .zip(values.chunks(MINUTES_PER_DAY))
            .map(|(counter, minutes)| {
                let counts = minutes.iter().map(|count| count.unwrap_or(0)).collect();
                (counter.to_string(), counts)
            })
            .collect();
        Self { day, by_counter }
    }

    pub fn day(&self) -> i64 {
        self.day
    }

    pub fn total(&self, counter: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> i64 {
        let Some(minutes) = self.by_counter.get(counter) else {
            return 0;
        };
        let first = self.minute_index(from);
        let end = self.minute_index(to);
        minutes
            .get(first..end)
            .map_or(0, |counts| counts.iter().sum())
    }

    fn minute_index(&self, at: DateTime<Utc>) -> usize {
        let minute = first_minute_starting_at_or_after(at) - self.day * MINUTES_PER_DAY as i64;
        minute.clamp(0, MINUTES_PER_DAY as i64) as usize
    }
}

pub fn days_touched(from: DateTime<Utc>, to: DateTime<Utc>) -> Range<i64> {
    let first = from.timestamp_micros().div_euclid(MICROS_PER_DAY);
    if from >= to {
        return first..first;
    }
    let last = (to.timestamp_micros() - 1).div_euclid(MICROS_PER_DAY);
    first..last + 1
}

fn first_minute_starting_at_or_after(at: DateTime<Utc>) -> i64 {
    (at.timestamp_micros() + MICROS_PER_MINUTE - 1).div_euclid(MICROS_PER_MINUTE)
}

#[cfg(test)]
mod tests {
    use redis::{AsyncCommands, Client};

    use super::*;

    fn instant(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    #[test]
    fn a_slot_is_the_day_key_and_the_minute_of_that_day() {
        assert_eq!(
            SNAPSHOT_HISTORY.slot(instant("2026-10-14T00:01:59Z")),
            MinuteSlot {
                key: "snap:hist:20740".into(),
                minute: 1,
            }
        );
        assert_eq!(
            HYPIXEL_HISTORY.slot(instant("2026-10-14T23:59:00Z")),
            MinuteSlot {
                key: "hp:hist:20740".into(),
                minute: 1439,
            }
        );
    }

    #[test]
    fn a_field_is_the_counter_followed_by_the_minute() {
        let slot = SNAPSHOT_HISTORY.slot(instant("2026-10-14T12:30:00Z"));
        assert_eq!(slot.field(FIRST_SNAPSHOTS), "f750");
        assert_eq!(slot.field(CHANGED_SNAPSHOTS), "d750");
    }

    #[test]
    fn a_day_totals_the_minutes_that_start_inside_a_span() {
        let mut values = vec![None; 2 * MINUTES_PER_DAY];
        values[0] = Some(4);
        values[750] = Some(2);
        values[751] = Some(3);
        values[MINUTES_PER_DAY + 750] = Some(1);
        let day = DayCounts::new(20_740, &[HYPIXEL_REQUESTS, HYPIXEL_ERRORS], &values);
        let total = |counter, from, to| day.total(counter, instant(from), instant(to));
        assert_eq!(
            total(
                HYPIXEL_REQUESTS,
                "2026-10-14T12:30:00Z",
                "2026-10-14T12:32:00Z"
            ),
            5
        );
        assert_eq!(
            total(
                HYPIXEL_REQUESTS,
                "2026-10-14T12:30:01Z",
                "2026-10-14T12:32:00Z"
            ),
            3
        );
        assert_eq!(
            total(
                HYPIXEL_ERRORS,
                "2026-10-13T00:00:00Z",
                "2026-10-16T00:00:00Z"
            ),
            1
        );
        assert_eq!(
            total(
                HYPIXEL_REQUESTS,
                "2026-10-15T00:00:00Z",
                "2026-10-15T01:00:00Z"
            ),
            0
        );
        assert_eq!(
            total(
                FIRST_SNAPSHOTS,
                "2026-10-14T00:00:00Z",
                "2026-10-15T00:00:00Z"
            ),
            0
        );
    }

    #[test]
    fn a_span_reads_every_day_it_touches() {
        let days =
            |from: &str, to: &str| days_touched(instant(from), instant(to)).collect::<Vec<_>>();
        assert_eq!(
            days("2026-10-14T23:30:00Z", "2026-10-15T00:30:00Z"),
            vec![20_740, 20_741]
        );
        assert_eq!(
            days("2026-10-14T00:00:00Z", "2026-10-15T00:00:00Z"),
            vec![20_740]
        );
        assert!(days("2026-10-14T10:00:00Z", "2026-10-14T10:00:00Z").is_empty());
    }

    #[tokio::test]
    async fn record_counts_each_counter_and_expires_the_day() {
        let Ok(url) = std::env::var("REDIS_URL") else {
            return;
        };
        let mut conn = ConnectionManager::new(Client::open(url).unwrap())
            .await
            .unwrap();
        let at = instant("1970-01-02T00:03:00Z");
        let slot = SNAPSHOT_HISTORY.slot(at);
        let () = conn.del(&slot.key).await.unwrap();

        let counters = [FIRST_SNAPSHOTS, CHANGED_SNAPSHOTS];
        SNAPSHOT_HISTORY
            .record(&mut conn, at, &counters)
            .await
            .unwrap();
        SNAPSHOT_HISTORY
            .record(&mut conn, at, &counters[..1])
            .await
            .unwrap();

        let counts: HashMap<String, i64> = conn.hgetall(&slot.key).await.unwrap();
        let ttl: i64 = conn.ttl(&slot.key).await.unwrap();
        let () = conn.del(&slot.key).await.unwrap();
        assert_eq!(
            counts,
            HashMap::from([("f3".to_string(), 2), ("d3".to_string(), 1)])
        );
        assert!(ttl > 0 && ttl <= HISTORY_TTL_SECS, "{ttl}");
    }

    #[tokio::test]
    async fn read_days_returns_every_minute_of_each_day() {
        let Ok(url) = std::env::var("REDIS_URL") else {
            return;
        };
        let mut conn = ConnectionManager::new(Client::open(url).unwrap())
            .await
            .unwrap();
        let late = instant("1970-01-03T23:59:00Z");
        let early = instant("1970-01-04T00:00:00Z");
        let counters = [HYPIXEL_REQUESTS, HYPIXEL_ERRORS];
        let keys = [
            HYPIXEL_HISTORY.slot(late).key,
            HYPIXEL_HISTORY.slot(early).key,
        ];
        let () = conn.del(&keys).await.unwrap();
        for (at, recorded) in [
            (late, &counters[..]),
            (late, &counters[..1]),
            (early, &counters[..1]),
        ] {
            HYPIXEL_HISTORY
                .record(&mut conn, at, recorded)
                .await
                .unwrap();
        }
        let days: Vec<i64> = days_touched(late, instant("1970-01-04T00:01:00Z")).collect();
        let read = HYPIXEL_HISTORY
            .read_days(&mut conn, &days, &counters)
            .await
            .unwrap();
        let () = conn.del(&keys).await.unwrap();
        let span_end = instant("1970-01-04T00:01:00Z");
        let total = |counter| -> i64 {
            read.iter()
                .map(|day| day.total(counter, late, span_end))
                .sum()
        };
        assert_eq!(days, vec![2, 3]);
        assert_eq!(total(HYPIXEL_REQUESTS), 3);
        assert_eq!(total(HYPIXEL_ERRORS), 1);
        assert_eq!(read[1].total(HYPIXEL_REQUESTS, early, span_end), 1);
    }
}
