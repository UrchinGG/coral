use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use database::Database;

use crate::coverage::Coverage;
use crate::error::{AdminError, Context};

pub const BATCH_ROUTE: &str = "/v3/players";
pub const BATCH_METHOD: &str = "POST";

const ROLLUP_HOUR: TimeDelta = TimeDelta::hours(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BatchSpan {
    head: (DateTime<Utc>, DateTime<Utc>),
    hours: (DateTime<Utc>, DateTime<Utc>),
    tail: (DateTime<Utc>, DateTime<Utc>),
}

pub async fn uncaptured_batches(
    db: &Database,
    coverage: &Coverage,
    from: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Option<i64>, AdminError> {
    let until = coverage.body_subjects_from.unwrap_or(now).min(now);
    if from >= until {
        return Ok(None);
    }
    if coverage.rollups_from.is_none_or(|since| since > from) {
        return Ok(None);
    }
    let span = BatchSpan::between(from, until)?;
    let count: i64 = sqlx::query_scalar(
        "SELECT (SELECT coalesce(sum(requests), 0) FROM api_traffic_hour
                 WHERE hour >= $3 AND hour < $4 AND route = $7 AND method = $8)
              + (SELECT coalesce(sum(requests), 0) FROM api_traffic_minute
                 WHERE ((minute >= $1 AND minute < $2) OR (minute >= $5 AND minute < $6))
                   AND route = $7 AND method = $8)",
    )
    .bind(span.head.0)
    .bind(span.head.1)
    .bind(span.hours.0)
    .bind(span.hours.1)
    .bind(span.tail.0)
    .bind(span.tail.1)
    .bind(BATCH_ROUTE)
    .bind(BATCH_METHOD)
    .fetch_one(db.pool())
    .await
    .context("count the batch lookups whose bodies were never captured")?;
    Ok(Some(count))
}

impl BatchSpan {
    fn between(from: DateTime<Utc>, until: DateTime<Utc>) -> Result<Self, AdminError> {
        let first_hour = ceil_hour(from)?;
        let last_hour = floor_hour(until)?;
        if first_hour >= last_hour {
            return Ok(Self {
                head: (from, until),
                hours: (until, until),
                tail: (until, until),
            });
        }
        Ok(Self {
            head: (from, first_hour),
            hours: (first_hour, last_hour),
            tail: (last_hour, until),
        })
    }
}

fn floor_hour(at: DateTime<Utc>) -> Result<DateTime<Utc>, AdminError> {
    at.duration_trunc(ROLLUP_HOUR).map_err(|error| {
        AdminError::Internal(format!("{at} cannot be snapped to an hour: {error}"))
    })
}

fn ceil_hour(at: DateTime<Utc>) -> Result<DateTime<Utc>, AdminError> {
    let floor = floor_hour(at)?;
    Ok(match floor == at {
        true => floor,
        false => floor + ROLLUP_HOUR,
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, 0).unwrap()
    }

    #[test]
    fn whole_hours_come_from_the_hour_table_and_the_edges_from_minutes() {
        assert_eq!(
            BatchSpan::between(at(1, 20), at(3, 14)).unwrap(),
            BatchSpan {
                head: (at(1, 20), at(2, 0)),
                hours: (at(2, 0), at(3, 0)),
                tail: (at(3, 0), at(3, 14)),
            }
        );
    }

    #[test]
    fn a_span_inside_one_hour_reads_only_minutes() {
        let span = BatchSpan::between(at(3, 1), at(3, 14)).unwrap();
        assert_eq!(span.head, (at(3, 1), at(3, 14)));
        assert_eq!(span.hours.0, span.hours.1);
        assert_eq!(span.tail.0, span.tail.1);
    }
}
