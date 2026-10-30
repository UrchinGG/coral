use chrono::{DateTime, TimeDelta, Utc};
use database::Database;
use serde::Serialize;
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::query::{LogRow, REQUEST_KEY};
use crate::refs::EntityRef;

pub const NEIGHBOR_SPAN: TimeDelta = TimeDelta::hours(24);
pub const SAME_PLAYER_SPAN: TimeDelta = TimeDelta::minutes(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Neighbors {
    pub prev_same_requester: Option<EntityRef>,
    pub next_same_requester: Option<EntityRef>,
    pub same_player_5m: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequesterScope {
    Member(i64),
    Actor(i64),
    Player(Uuid),
    KeyPrefix(String),
    Ip(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Older,
    Newer,
}

#[derive(FromRow)]
struct Neighbor {
    ts: DateTime<Utc>,
    request_key: Uuid,
}

impl RequesterScope {
    pub fn of(row: &LogRow) -> Option<Self> {
        if let Some(member_id) = row.member_id {
            return Some(Self::Member(member_id));
        }
        if let Some(actor_id) = row.actor_id {
            return Some(Self::Actor(actor_id));
        }
        if let Some(uuid) = row.actor_uuid {
            return Some(Self::Player(uuid));
        }
        if let Some(prefix) = &row.key_prefix {
            return Some(Self::KeyPrefix(prefix.clone()));
        }
        row.ip.clone().map(Self::Ip)
    }

    fn push(&self, sql: &mut QueryBuilder<'_, Postgres>) {
        match self {
            Self::Member(id) => sql.push("l.member_id = ").push_bind(*id),
            Self::Actor(id) => sql.push("l.actor_id = ").push_bind(*id),
            Self::Player(uuid) => sql.push("l.actor_uuid = ").push_bind(*uuid),
            Self::KeyPrefix(prefix) => sql
                .push("l.member_id IS NULL AND l.key_prefix = ")
                .push_bind(prefix.clone()),
            Self::Ip(ip) => sql.push("l.ip = ").push_bind(ip.clone()),
        };
    }
}

impl Direction {
    fn bounds(self, ts: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        match self {
            Self::Older => (ts - NEIGHBOR_SPAN, ts),
            Self::Newer => (ts, ts + NEIGHBOR_SPAN),
        }
    }

    fn comparison(self) -> &'static str {
        match self {
            Self::Older => "<",
            Self::Newer => ">",
        }
    }

    fn order(self) -> &'static str {
        match self {
            Self::Older => "DESC",
            Self::Newer => "ASC",
        }
    }
}

pub async fn neighbor(
    db: &Database,
    scope: Option<&RequesterScope>,
    row: &LogRow,
    direction: Direction,
) -> Result<Option<EntityRef>, AdminError> {
    let Some(scope) = scope else {
        return Ok(None);
    };
    let (from, until) = direction.bounds(row.ts);
    let comparison = direction.comparison();
    let order = direction.order();
    let mut sql = QueryBuilder::<Postgres>::new(format!(
        "SELECT l.ts, {REQUEST_KEY} AS request_key FROM api_request_log l WHERE "
    ));
    scope.push(&mut sql);
    sql.push(" AND l.ts >= ").push_bind(from);
    sql.push(" AND l.ts <= ").push_bind(until);
    sql.push(format!(" AND (l.ts {comparison} "))
        .push_bind(row.ts);
    sql.push(format!(" OR {REQUEST_KEY} {comparison} "))
        .push_bind(row.request_key)
        .push(")");
    sql.push(format!(
        " ORDER BY l.ts {order}, request_key {order} LIMIT 1"
    ));
    let found: Option<Neighbor> = sql
        .build_query_as()
        .fetch_optional(db.pool())
        .await
        .context("read the neighbouring request of the same requester")?;
    Ok(found.map(|neighbor| EntityRef::Request {
        request_id: neighbor.request_key,
        ts: neighbor.ts,
    }))
}

pub async fn same_player_requests(
    db: &Database,
    players: &[Uuid],
    row: &LogRow,
) -> Result<i64, AdminError> {
    if players.is_empty() {
        return Ok(0);
    }
    sqlx::query_scalar(
        "SELECT count(*) FROM (
           SELECT DISTINCT s.ts, s.request_id FROM api_request_subject s
           WHERE s.player = ANY($1) AND s.ts >= $2 AND s.ts <= $3
             AND NOT (s.ts = $4 AND s.request_id = $5)
         ) nearby",
    )
    .bind(players)
    .bind(row.ts - SAME_PLAYER_SPAN)
    .bind(row.ts + SAME_PLAYER_SPAN)
    .bind(row.ts)
    .bind(row.request_key)
    .fetch_one(db.pool())
    .await
    .context("count requests for the same players within five minutes")
}
