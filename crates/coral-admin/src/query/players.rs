use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use coral_redis::RedisPool;
use database::Database;
use moka::future::Cache;
use serde::Deserialize;
use sqlx::FromRow;
use uuid::Uuid;

use super::bounded::begin_bounded;
use crate::error::{AdminError, Context};
use crate::names::tracked_names;

pub const PLAYER_KEYS_CAPACITY: u64 = 10_000;
pub const PLAYER_KEYS_TTL: Duration = Duration::from_secs(30 * 60);
pub const EXPANSION_TIMEOUT: Duration = Duration::from_millis(800);
pub const MAX_UUIDS_PER_NAME: i64 = 10;

const IDENTITY_PREFIX: &str = "mj:id:";
const STALE_PREFIX: &str = "mj:stale:";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlayerInput {
    Uuid(Uuid),
    Name(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PlayerKeys {
    pub uuids: Vec<Uuid>,
    pub names: Vec<HeldName>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeldName {
    pub name: String,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct PlayerSearch {
    keys: Cache<PlayerInput, Arc<PlayerKeys>>,
    db: Arc<Database>,
    redis: RedisPool,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
struct NameSpan {
    uuid: String,
    name: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

#[derive(Deserialize)]
struct CachedIdentity {
    uuid: String,
}

struct NameOwners {
    current: Option<Uuid>,
    all: Vec<Uuid>,
}

impl PlayerInput {
    pub fn parse(raw: &str) -> Result<Self, AdminError> {
        let value = raw.trim();
        if let Some(uuid) = reqlog::parse_uuid(value) {
            return Ok(Self::Uuid(uuid));
        }
        if reqlog::is_player_name(value) {
            return Ok(Self::Name(value.to_ascii_lowercase()));
        }
        Err(AdminError::BadRequest(format!(
            "{value:?} is neither a player UUID nor a player name"
        )))
    }
}

impl HeldName {
    fn always(name: &str) -> Self {
        Self {
            name: name.to_string(),
            from: None,
            to: None,
        }
    }
}

impl PlayerSearch {
    pub fn new(db: Arc<Database>, redis: RedisPool) -> Self {
        Self {
            keys: Cache::builder()
                .max_capacity(PLAYER_KEYS_CAPACITY)
                .time_to_live(PLAYER_KEYS_TTL)
                .build(),
            db,
            redis,
        }
    }

    pub async fn keys(&self, input: &PlayerInput) -> Result<Arc<PlayerKeys>, AdminError> {
        self.keys
            .try_get_with(input.clone(), self.expand(input))
            .await
            .map_err(Arc::unwrap_or_clone)
    }

    async fn expand(&self, input: &PlayerInput) -> Result<Arc<PlayerKeys>, AdminError> {
        let keys = match input {
            PlayerInput::Uuid(uuid) => self.expand_uuid(*uuid).await?,
            PlayerInput::Name(name) => self.expand_name(name).await?,
        };
        Ok(Arc::new(keys))
    }

    async fn expand_uuid(&self, uuid: Uuid) -> Result<PlayerKeys, AdminError> {
        let hex = uuid.simple().to_string();
        let uuids = [hex.clone()];
        let (current, history) = tokio::try_join!(
            tracked_names(&self.redis, &uuids),
            name_history(&self.db, &uuids),
        )?;
        Ok(PlayerKeys {
            uuids: vec![uuid],
            names: held_names(
                history.get(&hex).map_or(&[][..], Vec::as_slice),
                current.get(&hex).map(String::as_str),
            ),
        })
    }

    async fn expand_name(&self, name: &str) -> Result<PlayerKeys, AdminError> {
        let owners = self.name_owners(name).await?;
        if owners.all.is_empty() {
            return Ok(PlayerKeys {
                uuids: Vec::new(),
                names: vec![HeldName::always(name)],
            });
        }
        let hexes: Vec<String> = owners
            .all
            .iter()
            .map(|uuid| uuid.simple().to_string())
            .collect();
        let (current, history) = tokio::try_join!(
            tracked_names(&self.redis, &hexes),
            name_history(&self.db, &hexes),
        )?;
        let mut names = Vec::new();
        for (uuid, hex) in owners.all.iter().zip(&hexes) {
            let current_name = current
                .get(hex)
                .map(String::as_str)
                .or_else(|| (owners.current == Some(*uuid)).then_some(name));
            let spans = history.get(hex).map_or(&[][..], Vec::as_slice);
            for held in held_names(spans, current_name) {
                if !names.contains(&held) {
                    names.push(held);
                }
            }
        }
        Ok(PlayerKeys {
            uuids: owners.all,
            names,
        })
    }

    async fn name_owners(&self, name: &str) -> Result<NameOwners, AdminError> {
        let (cached, snapshots) = tokio::try_join!(
            cached_identities(&self.redis, name),
            snapshot_owners(&self.db, name),
        )?;
        let [current, stale] = cached;
        let mut all: Vec<Uuid> = Vec::new();
        for uuid in current.iter().chain(stale.iter()).chain(snapshots.iter()) {
            if !all.contains(uuid) {
                all.push(*uuid);
            }
        }
        Ok(NameOwners { current, all })
    }
}

pub async fn cached_identities(
    redis: &RedisPool,
    name: &str,
) -> Result<[Option<Uuid>; 2], AdminError> {
    let keys = [
        format!("{IDENTITY_PREFIX}{name}"),
        format!("{STALE_PREFIX}{name}"),
    ];
    let values: Vec<Option<String>> = redis::cmd("MGET")
        .arg(&keys)
        .query_async(&mut redis.connection())
        .await
        .context("read cached name identities")?;
    let mut owners = values
        .into_iter()
        .map(|value| value.as_deref().and_then(cached_uuid));
    Ok([owners.next().flatten(), owners.next().flatten()])
}

fn cached_uuid(raw: &str) -> Option<Uuid> {
    let identity: CachedIdentity = serde_json::from_str(raw).ok()?;
    reqlog::parse_uuid(&identity.uuid)
}

async fn snapshot_owners(db: &Database, name: &str) -> Result<Vec<Uuid>, AdminError> {
    let mut tx = begin_bounded(db.pool(), EXPANSION_TIMEOUT).await?;
    let owners = sqlx::query_scalar(
        "SELECT DISTINCT uuid::uuid FROM player_snapshots WHERE lower(username) = $1 LIMIT $2",
    )
    .bind(name)
    .bind(MAX_UUIDS_PER_NAME)
    .fetch_all(&mut *tx)
    .await
    .context("find the players who held a name")?;
    tx.commit().await.context("finish the name lookup")?;
    Ok(owners)
}

async fn name_history(
    db: &Database,
    uuids: &[String],
) -> Result<HashMap<String, Vec<NameSpan>>, AdminError> {
    let mut tx = begin_bounded(db.pool(), EXPANSION_TIMEOUT).await?;
    let spans: Vec<NameSpan> = sqlx::query_as(
        "SELECT uuid::text AS uuid, lower(username) AS name,
                min(timestamp) AS first_seen, max(timestamp) AS last_seen
         FROM player_snapshots
         WHERE uuid = ANY($1) AND username IS NOT NULL
         GROUP BY uuid, lower(username)",
    )
    .bind(uuids)
    .fetch_all(&mut *tx)
    .await
    .context("read the players' name history")?;
    tx.commit().await.context("finish the name history")?;
    let mut by_uuid: HashMap<String, Vec<NameSpan>> = HashMap::new();
    for span in spans {
        by_uuid.entry(span.uuid.clone()).or_default().push(span);
    }
    Ok(by_uuid)
}

fn held_names(history: &[NameSpan], current: Option<&str>) -> Vec<HeldName> {
    let mut spans: Vec<&NameSpan> = history.iter().collect();
    spans.sort_by_key(|span| span.first_seen);
    let current = current
        .map(str::to_ascii_lowercase)
        .or_else(|| spans.last().map(|span| span.name.clone()));
    let mut held: Vec<HeldName> = spans
        .iter()
        .enumerate()
        .map(|(index, span)| HeldName {
            name: span.name.clone(),
            from: Some(span.first_seen),
            to: match current.as_deref() == Some(span.name.as_str()) {
                true => None,
                false => Some(
                    spans
                        .get(index + 1)
                        .map_or(span.last_seen, |next| next.first_seen),
                ),
            },
        })
        .collect();
    let Some(current) = current else {
        return held;
    };
    if !held.iter().any(|name| name.name == current) {
        held.push(HeldName {
            name: current,
            from: spans.iter().map(|span| span.last_seen).max(),
            to: None,
        });
    }
    held
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";

    fn day(d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, d, 0, 0, 0).unwrap()
    }

    fn span(name: &str, first: u32, last: u32) -> NameSpan {
        NameSpan {
            uuid: OJUGG.into(),
            name: name.into(),
            first_seen: day(first),
            last_seen: day(last),
        }
    }

    fn held(name: &str, from: Option<u32>, to: Option<u32>) -> HeldName {
        HeldName {
            name: name.into(),
            from: from.map(day),
            to: to.map(day),
        }
    }

    #[test]
    fn inputs_classify_as_uuid_in_any_form_or_lowercased_name() {
        let ojugg = Uuid::parse_str(OJUGG).unwrap();
        for raw in [
            OJUGG,
            "94589c87-999b-4a7a-b3ad-d2ae50711e64",
            " 94589C87999B4A7AB3ADD2AE50711E64 ",
        ] {
            assert_eq!(
                PlayerInput::parse(raw).unwrap(),
                PlayerInput::Uuid(ojugg),
                "{raw}"
            );
        }
        assert_eq!(
            PlayerInput::parse("oJugg").unwrap(),
            PlayerInput::Name("ojugg".into())
        );
        for raw in ["", "has space", "§edoorman", &"a".repeat(26)] {
            assert_eq!(
                PlayerInput::parse(raw).unwrap_err().code(),
                "bad_request",
                "{raw}"
            );
        }
    }

    #[test]
    fn each_name_is_held_until_the_next_name_appears() {
        let history = [
            span("gamma", 9, 12),
            span("alpha", 1, 3),
            span("beta", 4, 8),
        ];
        assert_eq!(
            held_names(&history, Some("Gamma")),
            vec![
                held("alpha", Some(1), Some(4)),
                held("beta", Some(4), Some(9)),
                held("gamma", Some(9), None),
            ]
        );
    }

    #[test]
    fn without_a_known_current_name_the_latest_name_stays_open() {
        let history = [span("alpha", 1, 3), span("beta", 4, 8)];
        assert_eq!(
            held_names(&history, None),
            vec![held("alpha", Some(1), Some(4)), held("beta", Some(4), None)]
        );
    }

    #[test]
    fn a_current_name_missing_from_history_starts_at_the_last_sighting() {
        let history = [span("alpha", 1, 3), span("beta", 4, 8)];
        assert_eq!(
            held_names(&history, Some("delta")),
            vec![
                held("alpha", Some(1), Some(4)),
                held("beta", Some(4), Some(8)),
                held("delta", Some(8), None),
            ]
        );
        assert_eq!(
            held_names(&[], Some("delta")),
            vec![held("delta", None, None)]
        );
        assert!(held_names(&[], None).is_empty());
    }

    #[test]
    fn cached_identities_yield_undashed_uuids() {
        assert_eq!(
            cached_uuid(r#"{"uuid":"94589c87-999b-4a7a-b3ad-d2ae50711e64","username":"oJugg"}"#),
            Uuid::try_parse(OJUGG).ok()
        );
        assert_eq!(cached_uuid("{}"), None);
    }

    async fn search() -> Option<(PlayerSearch, Arc<Database>, RedisPool)> {
        let db = Arc::new(
            Database::connect(&std::env::var("DATABASE_URL").ok()?)
                .await
                .ok()?,
        );
        let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
            .await
            .ok()?;
        Some((PlayerSearch::new(db.clone(), redis.clone()), db, redis))
    }

    async fn seed_snapshots(db: &Database, uuid: &str, names: &[(&str, &str)]) {
        sqlx::query("DELETE FROM player_snapshots WHERE uuid = $1")
            .bind(uuid)
            .execute(db.pool())
            .await
            .unwrap();
        for (name, at) in names {
            sqlx::query(
                "INSERT INTO player_snapshots (uuid, timestamp, username, is_baseline, data)
                 VALUES ($1, $2::timestamptz, $3, true, '{}')",
            )
            .bind(uuid)
            .bind(at)
            .bind(name)
            .execute(db.pool())
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn a_uuid_expands_to_its_names_and_held_windows() {
        let Some((search, db, _redis)) = search().await else {
            return;
        };
        let uuid = "7200000a999b4a7ab3add2ae50711e64";
        seed_snapshots(
            &db,
            uuid,
            &[
                ("FirstName", "2026-10-01T00:00:00Z"),
                ("SecondName", "2026-10-05T00:00:00Z"),
            ],
        )
        .await;

        let keys = search
            .keys(&PlayerInput::Uuid(Uuid::parse_str(uuid).unwrap()))
            .await
            .unwrap();

        assert_eq!(keys.uuids, vec![Uuid::parse_str(uuid).unwrap()]);
        assert_eq!(
            keys.names,
            vec![
                held("firstname", Some(1), Some(5)),
                held("secondname", Some(5), None),
            ]
        );
    }

    #[tokio::test]
    async fn a_name_expands_through_every_cache_and_snapshot_owner() {
        let Some((search, db, redis)) = search().await else {
            return;
        };
        let fresh = "7200000b999b4a7ab3add2ae50711e64";
        let former = "7200000c999b4a7ab3add2ae50711e64";
        seed_snapshots(
            &db,
            former,
            &[
                ("SharedName", "2026-10-02T00:00:00Z"),
                ("Moved", "2026-10-06T00:00:00Z"),
            ],
        )
        .await;
        seed_snapshots(&db, fresh, &[]).await;
        let _: () = redis::cmd("SET")
            .arg(format!("{IDENTITY_PREFIX}sharedname"))
            .arg(format!(r#"{{"uuid":"{fresh}","username":"SharedName"}}"#))
            .query_async(&mut redis.connection())
            .await
            .unwrap();

        let keys = search
            .keys(&PlayerInput::Name("sharedname".into()))
            .await
            .unwrap();

        assert_eq!(
            keys.uuids,
            vec![
                Uuid::parse_str(fresh).unwrap(),
                Uuid::parse_str(former).unwrap()
            ]
        );
        assert!(keys.names.contains(&held("sharedname", None, None)));
        assert!(keys.names.contains(&held("sharedname", Some(2), Some(6))));
        assert!(keys.names.contains(&held("moved", Some(6), None)));
    }

    #[tokio::test]
    async fn a_name_nobody_holds_counts_over_all_retention() {
        let Some((search, _db, _redis)) = search().await else {
            return;
        };
        let keys = search
            .keys(&PlayerInput::Name("nobody_has_this_name".into()))
            .await
            .unwrap();
        assert!(keys.uuids.is_empty());
        assert_eq!(keys.names, vec![held("nobody_has_this_name", None, None)]);
    }
}
