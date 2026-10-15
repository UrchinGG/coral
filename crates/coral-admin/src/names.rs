use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use chrono::Utc;
use clients::{ClientError, MojangClient};
use coral_redis::{RateLimiter, RedisPool};
use database::{Database, DiscordUsernameCacheRepository};
use moka::future::Cache;
use serde::Deserialize;
use serenity::all::{Http, UserId};
use serenity::http::HttpError;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};

use crate::error::{AdminError, Context};
use crate::refs::{Labels, PendingLabels};

pub const MINECRAFT_QUEUE: usize = 2_000;
pub const DISCORD_QUEUE: usize = 2_000;
pub const MOJANG_WORKER_INTERVAL: Duration = Duration::from_secs(2);
pub const MOJANG_BACKOFF: Duration = Duration::from_secs(5 * 60);
pub const MOJANG_ADMIN_BUCKET: &str = "mojang:admin";
pub const MOJANG_ADMIN_BUDGET: i64 = 150;
pub const MOJANG_SEARCH_RESERVE: i64 = 30;
pub const MOJANG_WORKER_BUDGET: i64 = MOJANG_ADMIN_BUDGET - MOJANG_SEARCH_RESERVE;
pub const DISCORD_WORKER_INTERVAL: Duration = Duration::from_millis(500);
pub const NEGATIVE_TTL: Duration = Duration::from_secs(3600);
pub const FETCHED_NAME_TTL: Duration = Duration::from_secs(3600);
pub const DISCORD_REFRESH_AFTER: chrono::TimeDelta = chrono::TimeDelta::days(7);

const TRACKED_NAME_PREFIX: &str = "mj:uuid:";
const UNKNOWN_USER_STATUS: u16 = 404;
const MAX_REMEMBERED: u64 = 50_000;

#[derive(Clone)]
pub struct NameService {
    names: Arc<Names>,
}

pub struct NameWorkers {
    names: Arc<Names>,
    minecraft: mpsc::Receiver<String>,
    discord: Option<(Arc<Http>, mpsc::Receiver<i64>)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelRequest {
    pub discord: Vec<i64>,
    pub minecraft: Vec<String>,
}

struct Names {
    db: Arc<Database>,
    redis: RedisPool,
    mojang: MojangClient,
    minecraft: ResolveQueue<String>,
    discord: Option<ResolveQueue<i64>>,
    fetched_minecraft: Cache<String, String>,
}

struct ResolveQueue<K> {
    sender: mpsc::Sender<K>,
    queued: Mutex<HashSet<K>>,
    failed: Cache<K, ()>,
    overflowing: AtomicBool,
}

struct Lookup<K> {
    found: BTreeMap<K, String>,
    pending: Vec<K>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Queueing {
    Never,
    Misses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Resolved,
    Failed,
}

enum MojangAttempt {
    Done(Outcome),
    Throttled,
    BackOff,
}

#[derive(Deserialize)]
struct TrackedName {
    username: String,
}

impl NameService {
    pub fn new(
        db: Arc<Database>,
        redis: RedisPool,
        discord: Option<Arc<Http>>,
    ) -> (Self, NameWorkers) {
        let (minecraft, minecraft_jobs) = ResolveQueue::new(MINECRAFT_QUEUE);
        let (discord_queue, discord_jobs) = match discord {
            Some(http) => {
                let (queue, jobs) = ResolveQueue::new(DISCORD_QUEUE);
                (Some(queue), Some((http, jobs)))
            }
            None => (None, None),
        };
        let names = Arc::new(Names {
            mojang: MojangClient::new(redis.connection()),
            db,
            redis,
            minecraft,
            discord: discord_queue,
            fetched_minecraft: Cache::builder()
                .max_capacity(MAX_REMEMBERED)
                .time_to_live(FETCHED_NAME_TTL)
                .build(),
        });
        let workers = NameWorkers {
            names: names.clone(),
            minecraft: minecraft_jobs,
            discord: discord_jobs,
        };
        (Self { names }, workers)
    }

    pub fn mojang(&self) -> &MojangClient {
        &self.names.mojang
    }

    pub async fn cached(&self, request: &LabelRequest) -> Result<Labels, AdminError> {
        self.labels(request, Queueing::Never).await
    }

    pub async fn resolve(&self, request: &LabelRequest) -> Result<Labels, AdminError> {
        self.labels(request, Queueing::Misses).await
    }

    pub fn fetches_discord(&self) -> bool {
        self.names.discord.is_some()
    }

    pub fn queue_discord(&self, ids: &[i64]) -> usize {
        let Some(queue) = &self.names.discord else {
            return 0;
        };
        ids.iter().filter(|id| queue.enqueue(**id)).count()
    }

    pub fn discord_queued(&self, ids: &[i64]) -> usize {
        self.names
            .discord
            .as_ref()
            .map_or(0, |queue| queue.queued_among(ids))
    }

    async fn labels(
        &self,
        request: &LabelRequest,
        queueing: Queueing,
    ) -> Result<Labels, AdminError> {
        let (discord, minecraft) = tokio::try_join!(
            self.discord_labels(&request.discord, queueing),
            self.minecraft_labels(&request.minecraft, queueing),
        )?;
        Ok(Labels {
            discord: discord.found,
            minecraft: minecraft.found,
            pending: PendingLabels {
                discord: discord.pending,
                minecraft: minecraft.pending,
            },
        })
    }

    async fn discord_labels(
        &self,
        ids: &[i64],
        queueing: Queueing,
    ) -> Result<Lookup<i64>, AdminError> {
        let ids = unique(ids);
        if ids.is_empty() {
            return Ok(Lookup::empty());
        }
        let cached = DiscordUsernameCacheRepository::new(self.names.db.pool())
            .get_many(&ids)
            .await
            .context("read cached Discord names")?;
        let refresh_before = Utc::now() - DISCORD_REFRESH_AFTER;
        let mut lookup = Lookup::empty();
        for id in ids {
            match cached.get(&id) {
                Some(entry) => {
                    if entry.last_refreshed < refresh_before && queueing == Queueing::Misses {
                        self.refresh_discord(id);
                    }
                    lookup.found.insert(id, entry.username.clone());
                }
                None if self.discord_will_resolve(id, queueing) => lookup.pending.push(id),
                None => {}
            }
        }
        Ok(lookup)
    }

    async fn minecraft_labels(
        &self,
        uuids: &[String],
        queueing: Queueing,
    ) -> Result<Lookup<String>, AdminError> {
        let mut misses = Vec::new();
        let mut lookup = Lookup::empty();
        for uuid in unique(uuids) {
            match self.names.fetched_minecraft.get(&uuid).await {
                Some(name) => {
                    lookup.found.insert(uuid, name);
                }
                None => misses.push(uuid),
            }
        }
        lookup
            .found
            .extend(tracked_names(&self.names.redis, &misses).await?);
        misses.retain(|uuid| !lookup.found.contains_key(uuid));
        lookup
            .found
            .extend(latest_snapshot_names(&self.names.db, &misses).await?);
        for uuid in misses {
            if !lookup.found.contains_key(&uuid) && self.minecraft_will_resolve(&uuid, queueing) {
                lookup.pending.push(uuid);
            }
        }
        Ok(lookup)
    }

    fn discord_will_resolve(&self, id: i64, queueing: Queueing) -> bool {
        self.names
            .discord
            .as_ref()
            .is_some_and(|queue| queue.will_resolve(id, queueing))
    }

    fn refresh_discord(&self, id: i64) {
        if let Some(queue) = &self.names.discord {
            queue.enqueue(id);
        }
    }

    fn minecraft_will_resolve(&self, uuid: &str, queueing: Queueing) -> bool {
        self.names
            .minecraft
            .will_resolve(uuid.to_string(), queueing)
    }
}

impl NameWorkers {
    pub fn spawn(self) {
        let NameWorkers {
            names,
            minecraft,
            discord,
        } = self;
        match discord {
            Some((http, jobs)) => {
                tokio::spawn(discord_worker(names.clone(), http, jobs));
            }
            None => tracing::warn!("DISCORD_TOKEN not configured; Discord names stay unresolved"),
        }
        tokio::spawn(mojang_worker(names, minecraft));
    }
}

impl<K: Clone + Eq + Hash + Send + Sync + 'static> ResolveQueue<K> {
    fn new(capacity: usize) -> (Self, mpsc::Receiver<K>) {
        let (sender, jobs) = mpsc::channel(capacity);
        let queue = Self {
            sender,
            queued: Mutex::new(HashSet::new()),
            failed: Cache::builder()
                .max_capacity(MAX_REMEMBERED)
                .time_to_live(NEGATIVE_TTL)
                .build(),
            overflowing: AtomicBool::new(false),
        };
        (queue, jobs)
    }

    fn will_resolve(&self, key: K, queueing: Queueing) -> bool {
        match queueing {
            Queueing::Never => !self.failed.contains_key(&key),
            Queueing::Misses => self.enqueue(key),
        }
    }

    fn enqueue(&self, key: K) -> bool {
        if self.failed.contains_key(&key) {
            return false;
        }
        let mut queued = self.queued.lock().unwrap_or_else(PoisonError::into_inner);
        if queued.contains(&key) {
            return true;
        }
        match self.sender.try_send(key.clone()) {
            Ok(()) => {
                self.overflowing.store(false, Ordering::Relaxed);
                queued.insert(key)
            }
            Err(_) => {
                if !self.overflowing.swap(true, Ordering::Relaxed) {
                    tracing::warn!(
                        capacity = self.sender.max_capacity(),
                        "name queue is full; new labels stay unresolved until it drains"
                    );
                }
                false
            }
        }
    }

    fn queued_among(&self, keys: &[K]) -> usize {
        let queued = self.queued.lock().unwrap_or_else(PoisonError::into_inner);
        keys.iter().filter(|key| queued.contains(*key)).count()
    }

    async fn finish(&self, key: &K, outcome: Outcome) {
        if outcome == Outcome::Failed {
            self.failed.insert(key.clone(), ()).await;
        }
        self.queued
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key);
    }
}

impl<K> Lookup<K> {
    fn empty() -> Self {
        Self {
            found: BTreeMap::new(),
            pending: Vec::new(),
        }
    }
}

pub async fn tracked_names(
    redis: &RedisPool,
    uuids: &[String],
) -> Result<HashMap<String, String>, AdminError> {
    if uuids.is_empty() {
        return Ok(HashMap::new());
    }
    let keys: Vec<String> = uuids
        .iter()
        .map(|uuid| format!("{TRACKED_NAME_PREFIX}{uuid}"))
        .collect();
    let values: Vec<Option<String>> = redis::cmd("MGET")
        .arg(&keys)
        .query_async(&mut redis.connection())
        .await
        .context("read tracked Minecraft names")?;
    Ok(uuids
        .iter()
        .zip(values)
        .filter_map(|(uuid, value)| Some((uuid.clone(), tracked_name(uuid, &value?)?)))
        .collect())
}

async fn latest_snapshot_names(
    db: &Database,
    uuids: &[String],
) -> Result<HashMap<String, String>, AdminError> {
    if uuids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT v.uuid, s.username
         FROM unnest($1::text[]) AS v(uuid)
         CROSS JOIN LATERAL (
           SELECT username FROM player_snapshots
           WHERE uuid = v.uuid AND username IS NOT NULL
           ORDER BY timestamp DESC
           LIMIT 1
         ) s",
    )
    .bind(uuids)
    .fetch_all(db.pool())
    .await
    .context("read the latest snapshot names")?;
    Ok(rows.into_iter().collect())
}

fn tracked_name(uuid: &str, raw: &str) -> Option<String> {
    match serde_json::from_str::<TrackedName>(raw) {
        Ok(tracked) => Some(tracked.username),
        Err(error) => {
            tracing::warn!(uuid, %error, "tracked Minecraft name is malformed; treated as a miss");
            None
        }
    }
}

fn unique<K: Clone + Ord>(keys: &[K]) -> Vec<K> {
    let mut keys = keys.to_vec();
    keys.sort_unstable();
    keys.dedup();
    keys
}

async fn mojang_worker(names: Arc<Names>, mut jobs: mpsc::Receiver<String>) {
    let limiter = RateLimiter::new(names.redis.clone());
    let mut pace = interval(MOJANG_WORKER_INTERVAL);
    pace.set_missed_tick_behavior(MissedTickBehavior::Delay);
    while let Some(uuid) = jobs.recv().await {
        loop {
            pace.tick().await;
            match fetch_minecraft_name(&names, &limiter, &uuid).await {
                MojangAttempt::Done(outcome) => {
                    names.minecraft.finish(&uuid, outcome).await;
                    break;
                }
                MojangAttempt::Throttled => {}
                MojangAttempt::BackOff => tokio::time::sleep(MOJANG_BACKOFF).await,
            }
        }
    }
}

async fn fetch_minecraft_name(names: &Names, limiter: &RateLimiter, uuid: &str) -> MojangAttempt {
    match limiter
        .try_acquire(MOJANG_ADMIN_BUCKET, MOJANG_WORKER_BUDGET)
        .await
    {
        Ok(true) => {}
        Ok(false) => return MojangAttempt::Throttled,
        Err(error) => {
            tracing::warn!(%error, "Mojang admin bucket unavailable; retrying");
            return MojangAttempt::Throttled;
        }
    }
    match names.mojang.resolve(uuid).await {
        Ok(identity) => {
            names
                .fetched_minecraft
                .insert(uuid.to_string(), identity.username)
                .await;
            MojangAttempt::Done(Outcome::Resolved)
        }
        Err(ClientError::RateLimited) => {
            tracing::warn!("Mojang rate limited the admin name worker; pausing");
            MojangAttempt::BackOff
        }
        Err(ClientError::PlayerNotFound(_)) => MojangAttempt::Done(Outcome::Failed),
        Err(error) => {
            tracing::warn!(uuid, %error, "Mojang name lookup failed");
            MojangAttempt::Done(Outcome::Failed)
        }
    }
}

async fn discord_worker(names: Arc<Names>, http: Arc<Http>, mut jobs: mpsc::Receiver<i64>) {
    let mut pace = interval(DISCORD_WORKER_INTERVAL);
    pace.set_missed_tick_behavior(MissedTickBehavior::Delay);
    while let Some(id) = jobs.recv().await {
        pace.tick().await;
        let outcome = fetch_discord_name(&names.db, &http, id).await;
        if let Some(queue) = &names.discord {
            queue.finish(&id, outcome).await;
        }
    }
}

async fn fetch_discord_name(db: &Database, http: &Http, id: i64) -> Outcome {
    let Some(user_id) = u64::try_from(id).ok().filter(|id| *id > 0).map(UserId::new) else {
        tracing::warn!(id, "not a Discord user id");
        return Outcome::Failed;
    };
    let user = match http.get_user(user_id).await {
        Ok(user) => user,
        Err(serenity::Error::Http(HttpError::UnsuccessfulRequest(response)))
            if response.status_code.as_u16() == UNKNOWN_USER_STATUS =>
        {
            tracing::debug!(id, "Discord user does not exist");
            return Outcome::Failed;
        }
        Err(error) => {
            tracing::warn!(id, %error, "Discord user lookup failed");
            return Outcome::Failed;
        }
    };
    match DiscordUsernameCacheRepository::new(db.pool())
        .upsert(id, &user.name)
        .await
    {
        Ok(_) => Outcome::Resolved,
        Err(error) => {
            tracing::warn!(id, %error, "Discord name could not be cached");
            Outcome::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";

    const UNUSED_TOKEN: &str = "never.used.token";

    async fn service(with_discord: bool) -> Option<(NameService, NameWorkers, Arc<Database>)> {
        let db = Arc::new(
            Database::connect(&std::env::var("DATABASE_URL").ok()?)
                .await
                .ok()?,
        );
        let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
            .await
            .ok()?;
        let discord = with_discord.then(|| Arc::new(Http::new(UNUSED_TOKEN.parse().unwrap())));
        let (service, workers) = NameService::new(db.clone(), redis, discord);
        Some((service, workers, db))
    }

    fn uuid(seed: u32) -> String {
        format!("{seed:08x}{}", &OJUGG[8..])
    }

    #[test]
    fn tracked_names_parse_the_mojang_cache_shape() {
        assert_eq!(
            tracked_name(OJUGG, r#"{"username":"oJugg","last_seen":1760000000}"#),
            Some("oJugg".to_string())
        );
        assert_eq!(tracked_name(OJUGG, "not json"), None);
    }

    #[test]
    fn unique_sorts_and_removes_repeats() {
        assert_eq!(unique(&[3, 1, 3, 2]), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn jobs_queue_discord_ids_once_and_see_them_drain() {
        let Some((with, _workers, _db)) = service(true).await else {
            return;
        };
        let ids = [7_100_000_001, 7_100_000_002];
        assert!(with.fetches_discord());
        assert_eq!(with.queue_discord(&ids), 2);
        assert_eq!(with.queue_discord(&ids), 2);
        assert_eq!(with.discord_queued(&[ids[0], ids[1], 7_100_000_003]), 2);
        let queue = with.names.discord.as_ref().unwrap();
        queue.finish(&ids[0], Outcome::Resolved).await;
        queue.finish(&ids[1], Outcome::Failed).await;
        assert_eq!(with.discord_queued(&ids), 0);
        assert_eq!(with.queue_discord(&ids), 1);

        let Some((without, _workers, _db)) = service(false).await else {
            return;
        };
        assert!(!without.fetches_discord());
        assert_eq!(without.queue_discord(&ids), 0);
        assert_eq!(without.discord_queued(&ids), 0);
    }

    #[tokio::test]
    async fn minecraft_labels_come_from_redis_then_snapshots() {
        let Some((service, _workers, db)) = service(false).await else {
            return;
        };
        let tracked = uuid(0x7100_0001);
        let snapshotted = uuid(0x7100_0002);
        let unknown = uuid(0x7100_0003);
        let _: () = redis::cmd("SET")
            .arg(format!("{TRACKED_NAME_PREFIX}{tracked}"))
            .arg(r#"{"username":"TrackedName","last_seen":1}"#)
            .query_async(&mut service.names.redis.connection())
            .await
            .unwrap();
        sqlx::query("DELETE FROM player_snapshots WHERE uuid = $1")
            .bind(&snapshotted)
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO player_snapshots (uuid, timestamp, username, is_baseline, data) VALUES
             ($1, now() - interval '2 days', 'OldName', true, '{}'),
             ($1, now() - interval '1 day', 'NewName', false, '{}'),
             ($1, now(), NULL, false, '{}')",
        )
        .bind(&snapshotted)
        .execute(db.pool())
        .await
        .unwrap();

        let request = LabelRequest {
            discord: vec![],
            minecraft: vec![
                tracked.clone(),
                snapshotted.clone(),
                unknown.clone(),
                tracked.clone(),
            ],
        };
        let cached = service.cached(&request).await.unwrap();

        assert_eq!(cached.minecraft[&tracked], "TrackedName");
        assert_eq!(cached.minecraft[&snapshotted], "NewName");
        assert_eq!(cached.pending.minecraft, vec![unknown.clone()]);
        assert!(
            !service
                .names
                .minecraft
                .queued
                .lock()
                .unwrap()
                .contains(&unknown)
        );

        let resolving = service.resolve(&request).await.unwrap();
        assert_eq!(resolving.pending.minecraft, vec![unknown.clone()]);
        assert!(
            service
                .names
                .minecraft
                .queued
                .lock()
                .unwrap()
                .contains(&unknown)
        );

        service
            .names
            .minecraft
            .finish(&unknown, Outcome::Failed)
            .await;
        let failed = service.resolve(&request).await.unwrap();
        assert!(failed.pending.minecraft.is_empty());
    }

    #[tokio::test]
    async fn discord_labels_read_the_cache_and_queue_only_on_resolve() {
        let Some((service, _workers, db)) = service(true).await else {
            return;
        };
        let cached_id = 920_000_000_000_000_001;
        let stale_id = 920_000_000_000_000_002;
        let missing_id = 920_000_000_000_000_003;
        sqlx::query("DELETE FROM discord_username_cache WHERE discord_id = ANY($1)")
            .bind([cached_id, stale_id, missing_id])
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO discord_username_cache (discord_id, username, last_refreshed) VALUES
             ($1, 'fresh', now()), ($2, 'stale', now() - interval '8 days')",
        )
        .bind(cached_id)
        .bind(stale_id)
        .execute(db.pool())
        .await
        .unwrap();
        let request = LabelRequest {
            discord: vec![cached_id, stale_id, missing_id],
            minecraft: vec![],
        };

        let cached = service.cached(&request).await.unwrap();
        assert_eq!(cached.discord[&cached_id], "fresh");
        assert_eq!(cached.discord[&stale_id], "stale");
        assert_eq!(cached.pending.discord, vec![missing_id]);
        let queue = service.names.discord.as_ref().unwrap();
        assert!(queue.queued.lock().unwrap().is_empty());

        let resolving = service.resolve(&request).await.unwrap();
        assert_eq!(resolving.pending.discord, vec![missing_id]);
        let queued = queue.queued.lock().unwrap().clone();
        assert_eq!(queued, HashSet::from([stale_id, missing_id]));
    }

    #[tokio::test]
    async fn without_a_bot_token_discord_misses_are_not_pending() {
        let Some((service, _workers, _db)) = service(false).await else {
            return;
        };
        let request = LabelRequest {
            discord: vec![920_000_000_000_000_099],
            minecraft: vec![],
        };
        let labels = service.resolve(&request).await.unwrap();
        assert!(labels.discord.is_empty());
        assert!(labels.pending.discord.is_empty());
    }

    #[tokio::test]
    async fn a_full_queue_leaves_ids_unqueued() {
        let (queue, _jobs) = ResolveQueue::<i64>::new(1);
        assert!(queue.enqueue(1));
        assert!(queue.enqueue(1));
        assert!(!queue.enqueue(2));
        assert!(queue.will_resolve(2, Queueing::Never));
        queue.finish(&1, Outcome::Resolved).await;
        assert!(!queue.queued.lock().unwrap().contains(&1));
    }
}
