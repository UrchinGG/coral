use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use coral_redis::RedisPool;
use redis::AsyncCommands;
use tokio::sync::Mutex;
use tokio::time::{self, Instant, MissedTickBehavior};

const EPOCH_KEY: &str = "admin:session:epoch";

const REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const STORE_TIMEOUT: Duration = Duration::from_secs(2);
const UNSET_EPOCH: u64 = 0;

static RAISE_TO: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r"
        local floor = tonumber(ARGV[1])
        if tonumber(redis.call('GET', KEYS[1]) or '0') < floor then
            redis.call('SET', KEYS[1], floor)
        end
        ",
    )
});

static ADVANCE_PAST: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r"
        local stored = tonumber(redis.call('GET', KEYS[1]) or '0')
        local advanced = math.max(stored, tonumber(ARGV[1])) + 1
        redis.call('SET', KEYS[1], advanced)
        return advanced
        ",
    )
});

#[async_trait]
pub trait EpochStore: Send + Sync {
    async fn read(&self) -> Result<u64>;
    async fn raise_to(&self, floor: u64) -> Result<()>;
    async fn advance_past(&self, floor: u64) -> Result<u64>;
}

pub struct SessionEpoch {
    current: AtomicU64,
    store: Arc<dyn EpochStore>,
    writer: Mutex<()>,
}

impl SessionEpoch {
    pub async fn load(store: Arc<dyn EpochStore>) -> Result<Self> {
        let current = bounded(store.read()).await?;
        Ok(Self {
            current: AtomicU64::new(current),
            store,
            writer: Mutex::new(()),
        })
    }

    pub fn current(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }

    pub async fn refresh(&self) -> Result<()> {
        let _writer = self.writer.lock().await;
        let current = self.current();
        let stored = bounded(self.store.read()).await?;
        if stored < current {
            tracing::warn!(
                "the stored session epoch fell from {current} to {stored}; restoring {current}"
            );
            return bounded(self.store.raise_to(current)).await;
        }
        self.current.store(stored, Ordering::Release);
        Ok(())
    }

    pub async fn revoke_all(&self) -> Result<()> {
        let _writer = self.writer.lock().await;
        let advanced = bounded(self.store.advance_past(self.current())).await?;
        self.current.store(advanced, Ordering::Release);
        Ok(())
    }

    pub fn keep_fresh(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = time::interval_at(Instant::now() + REFRESH_INTERVAL, REFRESH_INTERVAL);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                if let Err(error) = self.refresh().await {
                    tracing::warn!(
                        "session epoch refresh failed, keeping the last value: {error:#}"
                    );
                }
            }
        });
    }
}

#[async_trait]
impl EpochStore for RedisPool {
    async fn read(&self) -> Result<u64> {
        let stored: Option<u64> = self
            .connection()
            .get(EPOCH_KEY)
            .await
            .context("read the admin session epoch")?;
        Ok(stored.unwrap_or(UNSET_EPOCH))
    }

    async fn raise_to(&self, floor: u64) -> Result<()> {
        RAISE_TO
            .key(EPOCH_KEY)
            .arg(floor)
            .invoke_async(&mut self.connection())
            .await
            .context("restore the admin session epoch")
    }

    async fn advance_past(&self, floor: u64) -> Result<u64> {
        ADVANCE_PAST
            .key(EPOCH_KEY)
            .arg(floor)
            .invoke_async(&mut self.connection())
            .await
            .context("advance the admin session epoch")
    }
}

async fn bounded<T>(operation: impl Future<Output = Result<T>>) -> Result<T> {
    time::timeout(STORE_TIMEOUT, operation)
        .await
        .context("the session epoch store did not answer in time")?
}

#[cfg(test)]
pub mod tests {
    use std::future;

    use anyhow::bail;

    use super::*;

    #[derive(Clone, Copy, Default)]
    pub enum StoreHealth {
        #[default]
        Answering,
        Failing,
        Hanging,
    }

    #[derive(Default)]
    pub struct MemoryEpochStore {
        value: AtomicU64,
        health: std::sync::Mutex<StoreHealth>,
    }

    impl MemoryEpochStore {
        pub fn set(&self, value: u64) {
            self.value.store(value, Ordering::SeqCst);
        }

        pub fn set_health(&self, health: StoreHealth) {
            *self.health.lock().unwrap() = health;
        }

        async fn answer(&self) -> Result<()> {
            let health = *self.health.lock().unwrap();
            match health {
                StoreHealth::Answering => Ok(()),
                StoreHealth::Failing => bail!("epoch store unavailable"),
                StoreHealth::Hanging => future::pending().await,
            }
        }
    }

    #[async_trait]
    impl EpochStore for MemoryEpochStore {
        async fn read(&self) -> Result<u64> {
            self.answer().await?;
            Ok(self.value.load(Ordering::SeqCst))
        }

        async fn raise_to(&self, floor: u64) -> Result<()> {
            self.answer().await?;
            self.value.fetch_max(floor, Ordering::SeqCst);
            Ok(())
        }

        async fn advance_past(&self, floor: u64) -> Result<u64> {
            self.answer().await?;
            let advanced = self.value.load(Ordering::SeqCst).max(floor) + 1;
            self.value.store(advanced, Ordering::SeqCst);
            Ok(advanced)
        }
    }

    async fn epoch_with_store(initial: u64) -> (SessionEpoch, Arc<MemoryEpochStore>) {
        let store = Arc::new(MemoryEpochStore::default());
        store.set(initial);
        let epoch = SessionEpoch::load(store.clone()).await.unwrap();
        (epoch, store)
    }

    #[tokio::test]
    async fn load_starts_from_the_stored_epoch() {
        let (epoch, _) = epoch_with_store(7).await;
        assert_eq!(epoch.current(), 7);
    }

    #[tokio::test]
    async fn load_fails_when_the_store_is_unavailable() {
        let store = Arc::new(MemoryEpochStore::default());
        store.set_health(StoreHealth::Failing);
        assert!(SessionEpoch::load(store).await.is_err());
    }

    #[tokio::test]
    async fn revoke_all_advances_the_current_epoch_immediately() {
        let (epoch, store) = epoch_with_store(3).await;
        epoch.revoke_all().await.unwrap();
        assert_eq!(epoch.current(), 4);
        assert_eq!(store.read().await.unwrap(), 4);
    }

    #[tokio::test]
    async fn refresh_picks_up_a_revocation_made_elsewhere() {
        let (epoch, store) = epoch_with_store(0).await;
        store.advance_past(0).await.unwrap();
        assert_eq!(epoch.current(), 0);
        epoch.refresh().await.unwrap();
        assert_eq!(epoch.current(), 1);
    }

    #[tokio::test]
    async fn refresh_restores_a_stored_epoch_that_went_backwards() {
        let (epoch, store) = epoch_with_store(5).await;
        store.set(UNSET_EPOCH);
        epoch.refresh().await.unwrap();
        assert_eq!(epoch.current(), 5);
        assert_eq!(store.read().await.unwrap(), 5);
    }

    #[tokio::test]
    async fn revocation_moves_forward_even_after_the_stored_epoch_was_lost() {
        let (epoch, store) = epoch_with_store(5).await;
        store.set(UNSET_EPOCH);
        epoch.revoke_all().await.unwrap();
        assert_eq!(epoch.current(), 6);
        assert_eq!(store.read().await.unwrap(), 6);
    }

    #[tokio::test]
    async fn failed_refresh_keeps_the_last_epoch() {
        let (epoch, store) = epoch_with_store(5).await;
        store.set_health(StoreHealth::Failing);
        assert!(epoch.refresh().await.is_err());
        assert_eq!(epoch.current(), 5);
    }

    #[tokio::test]
    async fn failed_revocation_reports_an_error_and_keeps_the_epoch() {
        let (epoch, store) = epoch_with_store(5).await;
        store.set_health(StoreHealth::Failing);
        assert!(epoch.revoke_all().await.is_err());
        assert_eq!(epoch.current(), 5);
    }

    #[tokio::test(start_paused = true)]
    async fn keep_fresh_refreshes_within_the_interval() {
        let (epoch, store) = epoch_with_store(0).await;
        let epoch = Arc::new(epoch);
        epoch.clone().keep_fresh();
        store.set(9);
        time::sleep(REFRESH_INTERVAL + Duration::from_millis(1)).await;
        assert_eq!(epoch.current(), 9);
    }

    #[tokio::test(start_paused = true)]
    async fn hanging_store_fails_refresh_and_revocation_after_the_timeout() {
        let (epoch, store) = epoch_with_store(5).await;
        store.set_health(StoreHealth::Hanging);
        let started = Instant::now();
        assert!(epoch.refresh().await.is_err());
        assert!(epoch.revoke_all().await.is_err());
        assert!(started.elapsed() >= STORE_TIMEOUT * 2);
        assert_eq!(epoch.current(), 5);
    }
}
