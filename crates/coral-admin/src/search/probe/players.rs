use std::collections::HashMap;

use chrono::{DateTime, Utc};
use coral_redis::RedisPool;
use database::Database;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{Account, Found, ProbeContext, Sighting, bounded, finish, open};
use crate::error::{AdminError, Context};
use crate::names::tracked_names;
use crate::query::cached_identities;

pub const NAME_HOLDER_LIMIT: i64 = 10;
pub const NAME_HOLDER_SCAN_ROWS: i64 = 500;

#[derive(FromRow)]
struct OwnerRow {
    uuid: Uuid,
    discord_id: i64,
    member_id: i64,
}

#[derive(FromRow)]
struct HolderRow {
    uuid: Uuid,
    recent: bool,
    seen: DateTime<Utc>,
}

struct NameIdentities {
    current: Option<Uuid>,
    stale: Option<Uuid>,
}

pub(super) async fn player(
    db: &Database,
    uuid: Uuid,
    dashed: bool,
) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let known: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM players WHERE uuid = $1)")
        .bind(uuid.simple().to_string())
        .fetch_one(&mut *tx)
        .await
        .context("look up the player")?;
    let owners = owners_of(&mut tx, &[uuid])
        .await?
        .remove(&uuid)
        .unwrap_or_default();
    finish(tx).await?;
    if !known && owners.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Found::Player {
        uuid,
        dashed,
        owners,
    }])
}

pub(super) async fn player_requests(
    context: &ProbeContext<'_>,
    uuid: Uuid,
) -> Result<Vec<Found>, AdminError> {
    let Some(since) = context.coverage.searchable_subjects_from() else {
        return Ok(Vec::new());
    };
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let last: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT ts FROM api_request_subject
         WHERE player = $1 AND ts >= $2
         ORDER BY ts DESC LIMIT 1",
    )
    .bind(uuid)
    .bind(since)
    .fetch_optional(&mut *tx)
    .await
    .context("look for requests naming the UUID")?;
    finish(tx).await?;
    Ok(last
        .map(|last| Found::PlayerRequests { uuid, last })
        .into_iter()
        .collect())
}

pub(super) async fn name_holders(
    context: &ProbeContext<'_>,
    name: &str,
    history: bool,
) -> Result<Vec<Found>, AdminError> {
    let [current, stale] = cached_identities(context.redis, name).await?;
    let identities = NameIdentities { current, stale };
    if !history && current.is_none() && stale.is_none() {
        return Ok(Vec::new());
    }
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let sightings = match history {
        true => snapshot_holders(&mut tx, name).await?,
        false => Vec::new(),
    };
    let uuids = holder_uuids(&identities, &sightings);
    if uuids.is_empty() {
        finish(tx).await?;
        return Ok(Vec::new());
    }
    let current_names = current_names(context.redis, &mut tx, &uuids).await?;
    let mut owners = owners_of(&mut tx, &uuids).await?;
    finish(tx).await?;
    let mut sightings: HashMap<Uuid, Sighting> = sightings.into_iter().collect();
    Ok(uuids
        .into_iter()
        .map(|uuid| Found::NameHolder {
            uuid,
            name: name.to_string(),
            current: holds_now(&identities, &current_names, uuid, name),
            sighting: sightings.remove(&uuid),
            owners: owners.remove(&uuid).unwrap_or_default(),
        })
        .collect())
}

pub(super) async fn named_requests(
    context: &ProbeContext<'_>,
    name: &str,
) -> Result<Vec<Found>, AdminError> {
    let Some(since) = context.coverage.searchable_subjects_from() else {
        return Ok(Vec::new());
    };
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let last: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT ts FROM api_request_subject
         WHERE name = $1 AND player IS NULL AND ts >= $2
         ORDER BY ts DESC LIMIT 1",
    )
    .bind(name)
    .bind(since)
    .fetch_optional(&mut *tx)
    .await
    .context("look for requests naming the name")?;
    finish(tx).await?;
    Ok(last
        .map(|last| Found::NamedRequests {
            name: name.to_string(),
            last,
        })
        .into_iter()
        .collect())
}

pub(super) async fn owners_of(
    conn: &mut PgConnection,
    uuids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<Account>>, AdminError> {
    let hexes: Vec<String> = uuids.iter().map(|uuid| uuid.simple().to_string()).collect();
    let rows: Vec<OwnerRow> = sqlx::query_as(
        "SELECT o.uuid::uuid AS uuid, m.discord_id, m.id AS member_id
         FROM (
           SELECT uuid, id AS member_id FROM members WHERE uuid = ANY($1)
           UNION SELECT uuid, member_id FROM minecraft_accounts WHERE uuid = ANY($1)
           UNION SELECT uuid, member_id FROM member_mc_links
                 WHERE uuid = ANY($1) AND unlinked_at IS NULL
         ) o
         JOIN members m ON m.id = o.member_id
         ORDER BY m.id",
    )
    .bind(&hexes)
    .fetch_all(conn)
    .await
    .context("find the accounts that own the players")?;
    let mut owners: HashMap<Uuid, Vec<Account>> = HashMap::new();
    for row in rows {
        owners
            .entry(row.uuid)
            .or_default()
            .push(Account::member(row.discord_id, row.member_id));
    }
    Ok(owners)
}

fn holder_uuids(identities: &NameIdentities, sightings: &[(Uuid, Sighting)]) -> Vec<Uuid> {
    let mut uuids: Vec<Uuid> = Vec::new();
    let candidates = [identities.current, identities.stale]
        .into_iter()
        .flatten()
        .chain(sightings.iter().map(|(uuid, _)| *uuid));
    for uuid in candidates {
        if !uuids.contains(&uuid) {
            uuids.push(uuid);
        }
    }
    uuids
}

fn holds_now(
    identities: &NameIdentities,
    current_names: &HashMap<Uuid, String>,
    uuid: Uuid,
    name: &str,
) -> bool {
    match identities.current {
        Some(owner) => owner == uuid,
        None => current_names
            .get(&uuid)
            .is_some_and(|current| current.eq_ignore_ascii_case(name)),
    }
}

async fn snapshot_holders(
    conn: &mut PgConnection,
    name: &str,
) -> Result<Vec<(Uuid, Sighting)>, AdminError> {
    let rows: Vec<HolderRow> = sqlx::query_as(
        "SELECT uuid::uuid AS uuid, bool_or(newest) AS recent,
                coalesce(max(timestamp) FILTER (WHERE newest), min(timestamp)) AS seen
         FROM (
           (SELECT uuid, timestamp, true AS newest FROM player_snapshots
            WHERE lower(username) = $1 ORDER BY timestamp DESC LIMIT $2)
           UNION ALL
           (SELECT uuid, timestamp, false FROM player_snapshots
            WHERE lower(username) = $1 ORDER BY timestamp LIMIT $2)
         ) scanned
         GROUP BY uuid
         ORDER BY max(timestamp) DESC
         LIMIT $3",
    )
    .bind(name)
    .bind(NAME_HOLDER_SCAN_ROWS)
    .bind(NAME_HOLDER_LIMIT)
    .fetch_all(conn)
    .await
    .context("find the players whose snapshots carry the name")?;
    Ok(rows.into_iter().map(sighting_of).collect())
}

fn sighting_of(row: HolderRow) -> (Uuid, Sighting) {
    let sighting = match row.recent {
        true => Sighting::Last(row.seen),
        false => Sighting::First(row.seen),
    };
    (row.uuid, sighting)
}

async fn current_names(
    redis: &RedisPool,
    conn: &mut PgConnection,
    uuids: &[Uuid],
) -> Result<HashMap<Uuid, String>, AdminError> {
    let hexes: Vec<String> = uuids.iter().map(|uuid| uuid.simple().to_string()).collect();
    let tracked = tracked_names(redis, &hexes).await?;
    let untracked: Vec<String> = hexes
        .iter()
        .filter(|hex| !tracked.contains_key(*hex))
        .cloned()
        .collect();
    let snapshotted: HashMap<String, String> = match untracked.is_empty() {
        true => HashMap::new(),
        false => sqlx::query_as::<_, (String, String)>(
            "SELECT v.uuid, s.username
             FROM unnest($1::text[]) AS v(uuid)
             CROSS JOIN LATERAL (
               SELECT username FROM player_snapshots
               WHERE uuid = v.uuid AND username IS NOT NULL
               ORDER BY timestamp DESC
               LIMIT 1
             ) s",
        )
        .bind(&untracked)
        .fetch_all(conn)
        .await
        .context("read the latest snapshot names of the name's holders")?
        .into_iter()
        .collect(),
    };
    Ok(uuids
        .iter()
        .zip(&hexes)
        .filter_map(|(uuid, hex)| {
            let name = tracked.get(hex).or_else(|| snapshotted.get(hex))?;
            Some((*uuid, name.clone()))
        })
        .collect())
}
