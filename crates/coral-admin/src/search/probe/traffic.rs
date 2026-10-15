use std::net::IpAddr;
use std::ops::Range;

use chrono::{DateTime, TimeDelta, Utc};
use database::Database;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{Found, IpTraffic, bounded, finish, open};
use crate::error::{AdminError, Context};
use crate::search::input::is_time_ordered;

const REQUEST_ID_WINDOW: TimeDelta = TimeDelta::seconds(1);
const TIMESTAMP_RESOLUTION: TimeDelta = TimeDelta::microseconds(1);

#[derive(FromRow)]
struct RequestRow {
    ts: DateTime<Utc>,
    method: String,
    target: String,
    status: i16,
}

#[derive(FromRow)]
struct IpRow {
    accounts: i64,
    requests: i64,
    country: Option<String>,
    proxied: Option<bool>,
    last_seen: Option<DateTime<Utc>>,
}

pub(super) async fn request(db: &Database, request_id: Uuid) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let window = match time_ordered_instant(&request_id) {
        Some(at) => Some(at..at + REQUEST_ID_WINDOW),
        None => stored_instant(&mut tx, request_id)
            .await?
            .map(|ts| ts..ts + TIMESTAMP_RESOLUTION),
    };
    let row = match window {
        Some(window) => logged_request(&mut tx, request_id, window).await?,
        None => None,
    };
    finish(tx).await?;
    Ok(row
        .map(|row| Found::Request {
            request_id,
            ts: row.ts,
            method: row.method,
            target: row.target,
            status: row.status,
        })
        .into_iter()
        .collect())
}

pub(super) async fn ip_traffic(db: &Database, ip: IpAddr) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let row: IpRow = sqlx::query_as(
        "SELECT count(*) AS accounts, coalesce(sum(requests), 0)::bigint AS requests,
                (array_agg(country ORDER BY last_seen DESC) FILTER (WHERE country IS NOT NULL))[1]
                  AS country,
                bool_or(proxied) AS proxied, max(last_seen) AS last_seen
         FROM api_key_ips WHERE ip_address = $1::inet",
    )
    .bind(ip.to_string())
    .fetch_one(&mut *tx)
    .await
    .context("look up traffic from the IP")?;
    finish(tx).await?;
    let traffic = row.last_seen.map(|last_seen| IpTraffic {
        accounts: row.accounts,
        requests: row.requests,
        country: row.country,
        proxied: row.proxied.unwrap_or_default(),
        last_seen,
    });
    Ok(vec![Found::Ip { ip, traffic }])
}

async fn stored_instant(
    conn: &mut PgConnection,
    request_id: Uuid,
) -> Result<Option<DateTime<Utc>>, AdminError> {
    sqlx::query_scalar("SELECT ts FROM api_request_detail WHERE request_id = $1 LIMIT 1")
        .bind(request_id)
        .fetch_optional(conn)
        .await
        .context("look up the request's stored detail")
}

async fn logged_request(
    conn: &mut PgConnection,
    request_id: Uuid,
    window: Range<DateTime<Utc>>,
) -> Result<Option<RequestRow>, AdminError> {
    sqlx::query_as(
        "SELECT ts, method, coalesce(route, path) AS target,
                coalesce(outcome_status, status) AS status
         FROM api_request_log
         WHERE ts >= $2 AND ts < $3 AND request_id = $1
         LIMIT 1",
    )
    .bind(request_id)
    .bind(window.start)
    .bind(window.end)
    .fetch_optional(conn)
    .await
    .context("look up the request in the log")
}

fn time_ordered_instant(id: &Uuid) -> Option<DateTime<Utc>> {
    if !is_time_ordered(id) {
        return None;
    }
    let (seconds, nanos) = id.get_timestamp()?.to_unix();
    DateTime::from_timestamp(i64::try_from(seconds).ok()?, nanos)
}
