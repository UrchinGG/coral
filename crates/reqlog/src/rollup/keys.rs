use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use super::Span;
use crate::entry::KeyKind;

/// One key kind's requests per step across a span, zero where it was idle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeySeries {
    pub key_kind: KeyKind,
    pub values: Vec<i64>,
}

#[derive(FromRow)]
struct KeyBucket {
    t: DateTime<Utc>,
    key_kind: i16,
    requests: i64,
}

/// Requests an account made, per key kind, from the hourly caller rollup.
pub async fn requests_by_key(
    pool: &PgPool,
    member_id: Option<i64>,
    actor_id: i64,
    span: Span,
    step: TimeDelta,
) -> sqlx::Result<Vec<KeySeries>> {
    let buckets: Vec<KeyBucket> = sqlx::query_as(
        "SELECT date_bin($5::int8 * interval '1 second', hour, $3) AS t,
                key_kind, sum(requests)::int8 AS requests
         FROM api_caller_hour
         WHERE (member_id = $1 OR actor_id = $2) AND hour >= $3 AND hour < $4
         GROUP BY 1, 2",
    )
    .bind(member_id)
    .bind(actor_id)
    .bind(span.from)
    .bind(span.to)
    .bind(step.num_seconds())
    .fetch_all(pool)
    .await?;
    Ok(spread(&buckets, span, step))
}

fn spread(buckets: &[KeyBucket], span: Span, step: TimeDelta) -> Vec<KeySeries> {
    let steps = ((span.to - span.from).num_seconds() / step.num_seconds()).max(0) as usize;
    let mut kinds: BTreeMap<i16, Vec<i64>> = BTreeMap::new();
    for bucket in buckets {
        let index = ((bucket.t - span.from).num_seconds() / step.num_seconds()) as usize;
        let values = kinds
            .entry(bucket.key_kind)
            .or_insert_with(|| vec![0; steps]);
        if let Some(slot) = values.get_mut(index) {
            *slot += bucket.requests;
        }
    }
    kinds
        .into_iter()
        .filter_map(|(code, values)| {
            Some(KeySeries {
                key_kind: KeyKind::from_code(code)?,
                values,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::rollup::HOUR;

    #[test]
    fn buckets_land_on_their_step_per_key_kind() {
        let from = Utc.with_ymd_and_hms(2026, 11, 16, 0, 0, 0).unwrap();
        let span = Span::new(from, from + TimeDelta::hours(3)).unwrap();
        let bucket = |hours, kind: KeyKind, requests| KeyBucket {
            t: from + TimeDelta::hours(hours),
            key_kind: kind.code(),
            requests,
        };
        let series = spread(
            &[
                bucket(0, KeyKind::Personal, 4),
                bucket(2, KeyKind::Personal, 1),
                bucket(1, KeyKind::Developer, 9),
                bucket(5, KeyKind::Developer, 7),
            ],
            span,
            HOUR,
        );
        assert_eq!(
            series,
            vec![
                KeySeries {
                    key_kind: KeyKind::Personal,
                    values: vec![4, 0, 1],
                },
                KeySeries {
                    key_kind: KeyKind::Developer,
                    values: vec![0, 9, 0],
                },
            ]
        );
    }
}
