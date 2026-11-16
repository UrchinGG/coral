use std::collections::HashMap;

use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::forms::PlayerWindow;
use crate::error::{AdminError, Context};
use crate::query::PlayerInput;
use crate::serde_ts;
use crate::state::AppState;

const BUCKETS: i64 = 96;
const RATE_LIMITED: i16 = 429;
const SERVER_ERROR: i16 = 500;

const SERIES_SQL: &str = "WITH hits AS (
        SELECT s.ts, s.request_id, s.status FROM api_request_subject s
        WHERE s.player = $1 AND s.ts >= $2
        UNION
        SELECT s.ts, s.request_id, s.status
        FROM unnest($3::text[], $4::timestamptz[], $5::timestamptz[]) AS v(name, held_from, held_to)
        JOIN api_request_subject s ON s.player IS NULL AND s.name = v.name
         AND s.ts >= coalesce(v.held_from, '-infinity') AND s.ts < coalesce(v.held_to, 'infinity')
        WHERE s.ts >= $2
     )
     SELECT date_bin($6::interval, ts, $2) AS t,
            count(*) AS lookups,
            count(*) FILTER (WHERE status = $7) AS rate_limited,
            count(*) FILTER (WHERE status >= $8) AS errors
     FROM hits GROUP BY 1";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesParams {
    #[serde(default)]
    window: PlayerWindow,
}

#[derive(Serialize)]
pub struct LookupSeries {
    #[serde(serialize_with = "serde_ts::micros")]
    from: DateTime<Utc>,
    step_s: i64,
    clamped_from: Option<DateTime<Utc>>,
    series: Vec<LookupPoint>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, FromRow)]
pub struct LookupPoint {
    #[serde(serialize_with = "serde_ts::micros")]
    t: DateTime<Utc>,
    lookups: i64,
    rate_limited: i64,
    errors: i64,
}

/// How often a player was looked up, by the UUID or a name they held at the time,
/// bucketed across the window.
pub async fn read_series(
    state: &AppState,
    player: Uuid,
    params: SeriesParams,
) -> Result<LookupSeries, AdminError> {
    let step = params.window.length() / BUCKETS as i32;
    let now = Utc::now();
    let window_from = (now - params.window.length())
        .duration_trunc(step)
        .map_err(|error| AdminError::Internal(format!("align the lookup window: {error}")))?;
    let floor = state.coverage.current().searchable_subjects_from();
    let keys = state.players.keys(&PlayerInput::Uuid(player)).await?;
    let names: Vec<&str> = keys.names.iter().map(|held| held.name.as_str()).collect();
    let froms: Vec<Option<DateTime<Utc>>> = keys.names.iter().map(|held| held.from).collect();
    let tos: Vec<Option<DateTime<Utc>>> = keys.names.iter().map(|held| held.to).collect();
    let rows: Vec<LookupPoint> = sqlx::query_as(SERIES_SQL)
        .bind(player)
        .bind(window_from)
        .bind(&names)
        .bind(&froms)
        .bind(&tos)
        .bind(step)
        .bind(RATE_LIMITED)
        .bind(SERVER_ERROR)
        .fetch_all(state.db.pool())
        .await
        .context("count the player's lookups")?;
    Ok(LookupSeries {
        from: window_from,
        step_s: step.num_seconds(),
        clamped_from: floor.filter(|floor| *floor > window_from),
        series: fill(rows, window_from, step).collect(),
    })
}

fn fill(
    rows: Vec<LookupPoint>,
    from: DateTime<Utc>,
    step: TimeDelta,
) -> impl Iterator<Item = LookupPoint> {
    let mut found: HashMap<DateTime<Utc>, LookupPoint> =
        rows.into_iter().map(|point| (point.t, point)).collect();
    (0..BUCKETS).map(move |index| {
        let t = from + step * index as i32;
        found.remove(&t).unwrap_or(LookupPoint {
            t,
            ..LookupPoint::default()
        })
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn empty_buckets_are_filled_with_zeros_and_counted_ones_kept() {
        let from = Utc.with_ymd_and_hms(2026, 11, 15, 0, 0, 0).unwrap();
        let step = TimeDelta::minutes(15);
        let counted = LookupPoint {
            t: from + step * 2,
            lookups: 7,
            rate_limited: 1,
            errors: 0,
        };
        let points: Vec<LookupPoint> = fill(vec![counted], from, step).collect();
        assert_eq!(points.len(), BUCKETS as usize);
        assert_eq!(points[0].lookups, 0);
        assert_eq!(points[2], counted);
        assert_eq!(points[95].t, from + step * 95);
    }
}
