use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use database::{CacheRepository, Database};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::serde_ts;

pub const SNAPSHOT_PAGE_ROWS: i64 = 200;

const CURSOR_BYTES: usize = 16;
const MICROS_BYTES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotCursor {
    ts: DateTime<Utc>,
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotQuery {
    day: Option<NaiveDate>,
    cursor: Option<SnapshotCursor>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotAtQuery {
    ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotPage {
    pub days: Vec<SnapshotDay>,
    pub rows: Vec<SnapshotRow>,
    pub next_cursor: Option<SnapshotCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct SnapshotDay {
    pub day: NaiveDate,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct SnapshotRow {
    #[serde(skip)]
    pub id: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub ts: DateTime<Utc>,
    pub source: Option<String>,
    pub baseline: bool,
    pub username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotAt {
    #[serde(serialize_with = "serde_ts::micros")]
    pub ts: DateTime<Utc>,
    pub data: Value,
}

impl SnapshotCursor {
    fn encode(&self) -> String {
        let mut bytes = [0u8; CURSOR_BYTES];
        bytes[..MICROS_BYTES].copy_from_slice(&self.ts.timestamp_micros().to_be_bytes());
        bytes[MICROS_BYTES..].copy_from_slice(&self.id.to_be_bytes());
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn decode(raw: &str) -> Result<Self, AdminError> {
        let invalid = || AdminError::BadRequest(format!("cursor {raw:?} is not a snapshot cursor"));
        let bytes: [u8; CURSOR_BYTES] = URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| invalid())?
            .try_into()
            .map_err(|_| invalid())?;
        let (micros, id) = bytes.split_at(MICROS_BYTES);
        let micros = i64::from_be_bytes(micros.try_into().map_err(|_| invalid())?);
        let id = i64::from_be_bytes(id.try_into().map_err(|_| invalid())?);
        let ts = DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?;
        Ok(Self { ts, id })
    }
}

impl Serialize for SnapshotCursor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for SnapshotCursor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::decode(&raw).map_err(serde::de::Error::custom)
    }
}

pub async fn read_snapshots(
    db: &Database,
    player: Uuid,
    query: SnapshotQuery,
) -> Result<SnapshotPage, AdminError> {
    let uuid = player.simple().to_string();
    let (days, mut rows) = tokio::try_join!(
        snapshot_days(db, &uuid),
        snapshot_rows(db, &uuid, query.day, query.cursor),
    )?;
    let full = rows.len() as i64 > SNAPSHOT_PAGE_ROWS;
    rows.truncate(SNAPSHOT_PAGE_ROWS as usize);
    let next_cursor = full
        .then(|| rows.last())
        .flatten()
        .map(|row| SnapshotCursor {
            ts: row.ts,
            id: row.id,
        });
    Ok(SnapshotPage {
        days,
        rows,
        next_cursor,
    })
}

pub async fn read_snapshot_at(
    db: &Database,
    player: Uuid,
    query: SnapshotAtQuery,
) -> Result<SnapshotAt, AdminError> {
    let data = CacheRepository::new(db.pool())
        .get_snapshot_at(&player.simple().to_string(), query.ts)
        .await
        .context("rebuild the snapshot")?
        .ok_or_else(|| {
            AdminError::NotFound(format!("the player has no snapshot at {}", query.ts))
        })?;
    Ok(SnapshotAt { ts: query.ts, data })
}

async fn snapshot_days(db: &Database, uuid: &str) -> Result<Vec<SnapshotDay>, AdminError> {
    sqlx::query_as(
        "SELECT (timestamp AT TIME ZONE 'UTC')::date AS day, count(*) AS count
         FROM player_snapshots WHERE uuid = $1
         GROUP BY 1 ORDER BY 1 DESC",
    )
    .bind(uuid)
    .fetch_all(db.pool())
    .await
    .context("count the player's snapshots per day")
}

async fn snapshot_rows(
    db: &Database,
    uuid: &str,
    day: Option<NaiveDate>,
    cursor: Option<SnapshotCursor>,
) -> Result<Vec<SnapshotRow>, AdminError> {
    let (from, until) = day_bounds(day);
    sqlx::query_as(
        "SELECT id, timestamp AS ts, source, is_baseline AS baseline, username
         FROM player_snapshots
         WHERE uuid = $1
           AND ($2::timestamptz IS NULL OR timestamp >= $2)
           AND ($3::timestamptz IS NULL OR timestamp < $3)
           AND ($4::timestamptz IS NULL OR (timestamp, id) < ($4, $5))
         ORDER BY timestamp DESC, id DESC
         LIMIT $6",
    )
    .bind(uuid)
    .bind(from)
    .bind(until)
    .bind(cursor.map(|cursor| cursor.ts))
    .bind(cursor.map(|cursor| cursor.id))
    .bind(SNAPSHOT_PAGE_ROWS + 1)
    .fetch_all(db.pool())
    .await
    .context("list the player's snapshots")
}

fn day_bounds(day: Option<NaiveDate>) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let Some(start) = day.and_then(|day| day.and_hms_opt(0, 0, 0)) else {
        return (None, None);
    };
    let start = start.and_utc();
    (Some(start), Some(start + TimeDelta::days(1)))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn cursors_round_trip_and_reject_garbage() {
        let cursor = SnapshotCursor {
            ts: Utc.with_ymd_and_hms(2026, 10, 14, 3, 14, 32).unwrap()
                + TimeDelta::microseconds(17),
            id: 93_342,
        };
        assert_eq!(SnapshotCursor::decode(&cursor.encode()).unwrap(), cursor);
        assert_eq!(
            SnapshotCursor::decode("abc").unwrap_err().code(),
            "bad_request"
        );
    }

    #[test]
    fn a_day_spans_midnight_to_midnight_utc() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        assert_eq!(
            day_bounds(Some(day)),
            (
                Some(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()),
                Some(Utc.with_ymd_and_hms(2026, 10, 15, 0, 0, 0).unwrap())
            )
        );
        assert_eq!(day_bounds(None), (None, None));
    }

    async fn snapshotted(db: &Database, player: Uuid) -> String {
        let uuid = player.simple().to_string();
        sqlx::query("DELETE FROM player_snapshots WHERE uuid = $1")
            .bind(&uuid)
            .execute(db.pool())
            .await
            .unwrap();
        uuid
    }

    #[tokio::test]
    async fn snapshots_page_by_day_and_rebuild_from_the_baseline() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        let player = Uuid::parse_str("73000010000040008000000000000001").unwrap();
        let uuid = snapshotted(&db, player).await;
        let first_day = Utc.with_ymd_and_hms(2020, 2, 1, 0, 0, 0).unwrap();
        sqlx::query(
            "INSERT INTO player_snapshots (uuid, timestamp, source, username, is_baseline, data)
             SELECT $1, $2 + make_interval(mins => n), 'test', 'Snap', n = 0,
                    CASE WHEN n = 0 THEN '{\"stats\":{\"wins\":0},\"name\":\"Snap\"}'::jsonb
                         ELSE jsonb_build_object('stats', jsonb_build_object('wins', n)) END
             FROM generate_series(0, $3) AS n",
        )
        .bind(&uuid)
        .bind(first_day)
        .bind(SNAPSHOT_PAGE_ROWS as i32 + 4)
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO player_snapshots (uuid, timestamp, source, username, is_baseline, data)
             VALUES ($1, $2, 'test', 'Snap', false, '{\"stats\":{\"wins\":999}}')",
        )
        .bind(&uuid)
        .bind(first_day + TimeDelta::days(1))
        .execute(db.pool())
        .await
        .unwrap();

        let newest = read_snapshots(
            &db,
            player,
            SnapshotQuery {
                day: None,
                cursor: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            newest.days,
            vec![
                SnapshotDay {
                    day: NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    count: 1
                },
                SnapshotDay {
                    day: NaiveDate::from_ymd_opt(2020, 2, 1).unwrap(),
                    count: SNAPSHOT_PAGE_ROWS + 5
                },
            ]
        );
        assert_eq!(newest.rows.len() as i64, SNAPSHOT_PAGE_ROWS);
        assert_eq!(newest.rows[0].ts, first_day + TimeDelta::days(1));

        let day = NaiveDate::from_ymd_opt(2020, 2, 1).unwrap();
        let page = read_snapshots(
            &db,
            player,
            SnapshotQuery {
                day: Some(day),
                cursor: None,
            },
        )
        .await
        .unwrap();
        let rest = read_snapshots(
            &db,
            player,
            SnapshotQuery {
                day: Some(day),
                cursor: page.next_cursor,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            page.rows.len() + rest.rows.len(),
            SNAPSHOT_PAGE_ROWS as usize + 5
        );
        assert_eq!(rest.next_cursor, None);
        assert!(rest.rows.last().unwrap().baseline);

        let at = first_day + TimeDelta::minutes(3);
        let rebuilt = read_snapshot_at(&db, player, SnapshotAtQuery { ts: at })
            .await
            .unwrap();
        assert_eq!(
            rebuilt.data,
            serde_json::json!({ "stats": { "wins": 3 }, "name": "Snap" })
        );
        let missing = Uuid::parse_str("73000011000040008000000000000001").unwrap();
        assert_eq!(
            read_snapshot_at(&db, missing, SnapshotAtQuery { ts: at })
                .await
                .unwrap_err()
                .code(),
            "not_found"
        );
        snapshotted(&db, player).await;
    }
}
