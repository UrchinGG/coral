use std::panic::AssertUnwindSafe;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use coral_redis::RedisPool;
use database::Database;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use tokio::time::{MissedTickBehavior, interval};
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::query::begin_bounded;
use crate::serde_ts;

pub const JOB_KEY_PREFIX: &str = "admin:jobs:";
pub const JOB_STATUS_TTL_SECS: u64 = 30 * 86_400;
pub const JOB_SLICE_TIMEOUT: Duration = Duration::from_secs(120);
pub const QUIET_HOURS_START: u32 = 16;
pub const QUIET_HOURS_END: u32 = 7;
pub const QUIET_WINDOW_POLL: Duration = Duration::from_secs(60);
pub const DUMP_APPLICATION: &str = "pg_dump";
pub const JOB_LEASE_TTL: Duration = Duration::from_secs(30);
pub const JOB_LEASE_RENEWAL: Duration = Duration::from_secs(10);

const LEASE_SUFFIX: &str = ":lease";

static RENEW_LEASE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r"
        if redis.call('GET', KEYS[1]) == ARGV[1] then
            return redis.call('PEXPIRE', KEYS[1], ARGV[2])
        end
        return 0
        ",
    )
});

static RELEASE_LEASE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r"
        if redis.call('GET', KEYS[1]) == ARGV[1] then
            return redis.call('DEL', KEYS[1])
        end
        return 0
        ",
    )
});

#[async_trait]
pub trait Job: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: &JobContext) -> Result<(), AdminError>;
}

#[derive(Clone)]
pub struct JobRegistry {
    inner: Arc<Registry>,
}

pub struct JobContext {
    registry: Arc<Registry>,
    name: &'static str,
    started_at: DateTime<Utc>,
    progress: Mutex<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Waiting,
    Succeeded,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobStatus {
    pub name: String,
    pub instance: String,
    pub state: JobState,
    #[serde(serialize_with = "serde_ts::micros")]
    pub started_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub updated_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub finished_at: Option<DateTime<Utc>>,
    pub progress: Value,
    pub error: Option<String>,
}

struct Registry {
    db: Arc<Database>,
    redis: RedisPool,
    instance: String,
}

impl JobRegistry {
    pub fn new(db: Arc<Database>, redis: RedisPool) -> Self {
        Self {
            inner: Arc::new(Registry {
                db,
                redis,
                instance: Uuid::new_v4().simple().to_string(),
            }),
        }
    }

    pub async fn start(&self, job: Arc<dyn Job>) -> Result<JobStatus, AdminError> {
        let name = job.name();
        if !self.inner.take_lease(name).await? {
            return Err(AdminError::Rejected(format!(
                "job {name} is already running"
            )));
        }
        let ctx = JobContext::new(self.inner.clone(), name);
        let status = ctx.status(JobState::Running, None);
        if let Err(error) = self.inner.store(&status).await {
            self.inner.release_lease(name).await;
            return Err(error);
        }
        tokio::spawn(run_job(job, ctx));
        Ok(status)
    }

    pub async fn status(&self, name: &str) -> Result<Option<JobStatus>, AdminError> {
        let (raw, holder): (Option<String>, Option<String>) = redis::pipe()
            .get(job_key(name))
            .get(lease_key(name))
            .query_async(&mut self.inner.redis.connection())
            .await
            .context(format!("read {}", job_key(name)))?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let status = parse_status(name, &raw)?;
        Ok(Some(status.unless_abandoned(holder.as_deref())))
    }

    pub async fn is_running(&self, name: &str) -> Result<bool, AdminError> {
        redis::cmd("EXISTS")
            .arg(lease_key(name))
            .query_async(&mut self.inner.redis.connection())
            .await
            .context(format!("read {}", lease_key(name)))
    }
}

impl JobStatus {
    fn unless_abandoned(self, lease_holder: Option<&str>) -> Self {
        let active = matches!(self.state, JobState::Running | JobState::Waiting);
        match active && lease_holder != Some(self.instance.as_str()) {
            true => Self {
                state: JobState::Interrupted,
                ..self
            },
            false => self,
        }
    }
}

impl JobContext {
    fn new(registry: Arc<Registry>, name: &'static str) -> Self {
        Self {
            registry,
            name,
            started_at: Utc::now(),
            progress: Mutex::new(Value::Null),
        }
    }

    pub fn db(&self) -> &Database {
        &self.registry.db
    }

    pub fn redis(&self) -> &RedisPool {
        &self.registry.redis
    }

    pub async fn report(&self, progress: Value) -> Result<(), AdminError> {
        *self.lock_progress() = progress;
        self.store_state(JobState::Running).await
    }

    pub async fn slice(&self) -> Result<Transaction<'static, Postgres>, AdminError> {
        begin_bounded(self.registry.db.pool(), JOB_SLICE_TIMEOUT).await
    }

    pub async fn wait_for_quiet_window(&self) -> Result<(), AdminError> {
        if self.is_quiet().await? {
            return Ok(());
        }
        self.store_state(JobState::Waiting).await?;
        while !self.is_quiet().await? {
            tokio::time::sleep(QUIET_WINDOW_POLL).await;
        }
        self.store_state(JobState::Running).await
    }

    async fn is_quiet(&self) -> Result<bool, AdminError> {
        backfill_window_open(&self.registry.db, Utc::now()).await
    }

    async fn store_state(&self, state: JobState) -> Result<(), AdminError> {
        self.registry.store(&self.status(state, None)).await
    }

    fn status(&self, state: JobState, error: Option<String>) -> JobStatus {
        let now = Utc::now();
        let finished = matches!(state, JobState::Succeeded | JobState::Failed);
        JobStatus {
            name: self.name.to_string(),
            instance: self.registry.instance.clone(),
            state,
            started_at: self.started_at,
            updated_at: now,
            finished_at: finished.then_some(now),
            progress: self.lock_progress().clone(),
            error,
        }
    }

    fn lock_progress(&self) -> MutexGuard<'_, Value> {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn finish(self, outcome: Result<(), AdminError>) {
        let status = match outcome {
            Ok(()) => self.status(JobState::Succeeded, None),
            Err(error) => {
                tracing::error!(job = self.name, %error, "admin job failed");
                self.status(JobState::Failed, Some(error.to_string()))
            }
        };
        if let Err(error) = self.registry.store(&status).await {
            tracing::error!(job = self.name, %error, "admin job outcome not recorded");
        }
        self.registry.release_lease(self.name).await;
    }
}

impl Registry {
    async fn take_lease(&self, name: &str) -> Result<bool, AdminError> {
        let taken: Option<String> = redis::cmd("SET")
            .arg(lease_key(name))
            .arg(&self.instance)
            .arg("NX")
            .arg("PX")
            .arg(lease_ttl_millis())
            .query_async(&mut self.redis.connection())
            .await
            .context(format!("take {}", lease_key(name)))?;
        Ok(taken.is_some())
    }

    async fn renew_lease(&self, name: &str) -> Result<bool, AdminError> {
        let renewed: i64 = RENEW_LEASE
            .key(lease_key(name))
            .arg(&self.instance)
            .arg(lease_ttl_millis())
            .invoke_async(&mut self.redis.connection())
            .await
            .context(format!("renew {}", lease_key(name)))?;
        Ok(renewed == 1)
    }

    async fn release_lease(&self, name: &str) {
        let released: Result<i64, _> = RELEASE_LEASE
            .key(lease_key(name))
            .arg(&self.instance)
            .invoke_async(&mut self.redis.connection())
            .await;
        if let Err(error) = released {
            tracing::error!(job = name, %error, "admin job lease not released; it expires by itself");
        }
    }

    async fn keep_lease(self: Arc<Self>, name: &'static str) {
        let mut renewals = interval(JOB_LEASE_RENEWAL);
        renewals.set_missed_tick_behavior(MissedTickBehavior::Delay);
        renewals.tick().await;
        loop {
            renewals.tick().await;
            match self.renew_lease(name).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::error!(
                        job = name,
                        "admin job lost its lease and stops; another instance may start it"
                    );
                    return;
                }
                Err(error) => tracing::warn!(job = name, %error, "admin job lease not renewed"),
            }
        }
    }

    async fn store(&self, status: &JobStatus) -> Result<(), AdminError> {
        let raw = serde_json::to_string(status)
            .map_err(|error| AdminError::Internal(format!("encode job status: {error}")))?;
        redis::cmd("SET")
            .arg(job_key(&status.name))
            .arg(raw)
            .arg("EX")
            .arg(JOB_STATUS_TTL_SECS)
            .query_async::<()>(&mut self.redis.connection())
            .await
            .context(format!("write {}", job_key(&status.name)))
    }
}

pub fn is_quiet_hour(at: DateTime<Utc>) -> bool {
    let hour = at.hour();
    !(QUIET_HOURS_END..QUIET_HOURS_START).contains(&hour)
}

pub async fn backfill_window_open(db: &Database, at: DateTime<Utc>) -> Result<bool, AdminError> {
    if !is_quiet_hour(at) {
        return Ok(false);
    }
    let dumping: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE application_name = $1)",
    )
    .bind(DUMP_APPLICATION)
    .fetch_one(db.pool())
    .await
    .context("check for a running database dump")?;
    Ok(!dumping)
}

async fn run_job(job: Arc<dyn Job>, ctx: JobContext) {
    let name = ctx.name;
    let outcome = tokio::select! {
        outcome = AssertUnwindSafe(job.run(&ctx)).catch_unwind() => outcome
            .unwrap_or_else(|_| Err(AdminError::Internal(format!("job {name} panicked")))),
        () = ctx.registry.clone().keep_lease(name) => return,
    };
    ctx.finish(outcome).await;
}

fn job_key(name: &str) -> String {
    format!("{JOB_KEY_PREFIX}{name}")
}

fn lease_key(name: &str) -> String {
    format!("{JOB_KEY_PREFIX}{name}{LEASE_SUFFIX}")
}

fn lease_ttl_millis() -> u64 {
    JOB_LEASE_TTL.as_millis() as u64
}

fn parse_status(name: &str, raw: &str) -> Result<JobStatus, AdminError> {
    serde_json::from_str(raw).map_err(|error| {
        AdminError::Redis(format!(
            "{} holds a malformed status: {error}",
            job_key(name)
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use chrono::TimeZone;
    use serde_json::json;
    use tokio::sync::Notify;

    use super::*;

    struct Gated {
        name: &'static str,
        gate: Arc<Notify>,
        fail: bool,
    }

    #[async_trait]
    impl Job for Gated {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn run(&self, ctx: &JobContext) -> Result<(), AdminError> {
            ctx.report(json!({ "step": 1 })).await?;
            self.gate.notified().await;
            match self.fail {
                true => Err(AdminError::Timeout("slice 3".into())),
                false => Ok(()),
            }
        }
    }

    async fn registry() -> Option<JobRegistry> {
        let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?;
        let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
            .await
            .ok()?;
        Some(JobRegistry::new(Arc::new(db), redis))
    }

    async fn settled(registry: &JobRegistry, name: &str) -> JobStatus {
        for _ in 0..100 {
            let status = registry.status(name).await.unwrap().unwrap();
            if status.finished_at.is_some() && !registry.is_running(name).await.unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("job {name} did not finish");
    }

    #[test]
    fn quiet_hours_run_from_sixteen_to_seven_utc() {
        let at = |hour| Utc.with_ymd_and_hms(2026, 10, 14, hour, 0, 0).unwrap();
        for hour in [16, 23, 0, 6] {
            assert!(is_quiet_hour(at(hour)), "{hour}");
        }
        for hour in [7, 8, 15] {
            assert!(!is_quiet_hour(at(hour)), "{hour}");
        }
    }

    #[tokio::test]
    async fn jobs_report_progress_and_their_outcome() {
        let Some(registry) = registry().await else {
            return;
        };
        let gate = Arc::new(Notify::new());
        let name = "test-succeeds";
        let started = registry
            .start(Arc::new(Gated {
                name,
                gate: gate.clone(),
                fail: false,
            }))
            .await
            .unwrap();
        assert_eq!(started.state, JobState::Running);
        assert!(registry.is_running(name).await.unwrap());

        let second = registry
            .start(Arc::new(Gated {
                name,
                gate: gate.clone(),
                fail: false,
            }))
            .await;
        assert_eq!(second.unwrap_err().code(), "rejected");

        gate.notify_one();
        let finished = settled(&registry, name).await;
        assert_eq!(finished.state, JobState::Succeeded);
        assert_eq!(finished.progress, json!({ "step": 1 }));
        assert!(!registry.is_running(name).await.unwrap());
    }

    struct Panics;

    #[async_trait]
    impl Job for Panics {
        fn name(&self) -> &'static str {
            "test-panics"
        }

        async fn run(&self, _ctx: &JobContext) -> Result<(), AdminError> {
            panic!("slice exploded")
        }
    }

    #[tokio::test]
    async fn a_panicking_job_is_recorded_as_failed_and_can_run_again() {
        let Some(registry) = registry().await else {
            return;
        };
        registry.start(Arc::new(Panics)).await.unwrap();
        let finished = settled(&registry, "test-panics").await;
        assert_eq!(finished.state, JobState::Failed);
        assert_eq!(finished.error.as_deref(), Some("job test-panics panicked"));
        assert!(!registry.is_running("test-panics").await.unwrap());
    }

    #[tokio::test]
    async fn failed_jobs_keep_their_error() {
        let Some(registry) = registry().await else {
            return;
        };
        let gate = Arc::new(Notify::new());
        let name = "test-fails";
        registry
            .start(Arc::new(Gated {
                name,
                gate: gate.clone(),
                fail: true,
            }))
            .await
            .unwrap();
        gate.notify_one();
        let finished = settled(&registry, name).await;
        assert_eq!(finished.state, JobState::Failed);
        assert_eq!(finished.error.as_deref(), Some("slice 3"));
    }

    #[tokio::test]
    async fn slices_run_under_the_job_statement_timeout() {
        let Some(registry) = registry().await else {
            return;
        };
        let ctx = JobContext::new(registry.inner.clone(), "test-slice");
        let mut slice = ctx.slice().await.unwrap();
        let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(&mut *slice)
            .await
            .unwrap();
        assert_eq!(timeout, "2min");
    }

    #[tokio::test]
    async fn another_instance_cannot_start_a_job_that_is_running() {
        let (Some(first), Some(second)) = (registry().await, registry().await) else {
            return;
        };
        let gate = Arc::new(Notify::new());
        let name = "test-single-instance";
        let gated = || {
            Arc::new(Gated {
                name,
                gate: gate.clone(),
                fail: false,
            })
        };
        first.start(gated()).await.unwrap();

        let rejected = second.start(gated()).await.unwrap_err();
        assert_eq!(rejected.code(), "rejected");
        assert!(second.is_running(name).await.unwrap());

        gate.notify_one();
        settled(&first, name).await;
        second.start(gated()).await.unwrap();
        gate.notify_one();
        let finished = settled(&second, name).await;
        assert_eq!(finished.instance, second.inner.instance);
    }

    #[tokio::test]
    async fn a_running_job_whose_lease_lapsed_reads_as_interrupted() {
        let Some(registry) = registry().await else {
            return;
        };
        let ctx = JobContext::new(registry.inner.clone(), "test-interrupted");
        registry
            .inner
            .store(&ctx.status(JobState::Running, None))
            .await
            .unwrap();

        let status = registry.status("test-interrupted").await.unwrap().unwrap();

        assert_eq!(status.state, JobState::Interrupted);
        assert!(!registry.is_running("test-interrupted").await.unwrap());
    }

    struct Unending {
        stopped: Arc<AtomicBool>,
    }

    struct StopsOnDrop(Arc<AtomicBool>);

    impl Drop for StopsOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[async_trait]
    impl Job for Unending {
        fn name(&self) -> &'static str {
            "test-lease-lost"
        }

        async fn run(&self, _ctx: &JobContext) -> Result<(), AdminError> {
            let _running = StopsOnDrop(self.stopped.clone());
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn a_job_that_loses_its_lease_stops_and_reads_as_interrupted() {
        let Some(registry) = registry().await else {
            return;
        };
        let stopped = Arc::new(AtomicBool::new(false));
        let job = Unending {
            stopped: stopped.clone(),
        };
        let name = job.name();
        registry.start(Arc::new(job)).await.unwrap();
        redis::cmd("SET")
            .arg(lease_key(name))
            .arg("another-instance")
            .arg("PX")
            .arg(lease_ttl_millis())
            .query_async::<()>(&mut registry.inner.redis.connection())
            .await
            .unwrap();

        let deadline = tokio::time::Instant::now() + JOB_LEASE_RENEWAL * 3;
        while !stopped.load(Ordering::Acquire) && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        assert!(stopped.load(Ordering::Acquire));
        let status = registry.status(name).await.unwrap().unwrap();
        assert_eq!(status.state, JobState::Interrupted);
        redis::cmd("DEL")
            .arg(lease_key(name))
            .query_async::<()>(&mut registry.inner.redis.connection())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn unknown_jobs_have_no_status() {
        let Some(registry) = registry().await else {
            return;
        };
        assert_eq!(registry.status("never-started").await.unwrap(), None);
    }
}
