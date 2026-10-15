use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clients::Extension;
use coral_redis::RedisPool;
use database::{Database, PoolSettings};

use crate::auth::AuthConfig;
use crate::boot::BootSource;
use crate::coverage::CoverageCache;
use crate::discord::DiscordContext;
use crate::extensions;
use crate::flags::SpikeBaselines;
use crate::jobs::JobRegistry;
use crate::names::{NameService, NameWorkers};
use crate::players::PlayerLookups;
use crate::query::{PlayerSearch, ScanRates, SearchLimits};
use crate::rollups::Rollups;
use crate::traffic::history::UpstreamHistory;
use crate::views::ViewCache;

pub const INTERACTIVE_POOL: PoolSettings = PoolSettings {
    max_connections: 10,
    acquire_timeout: Duration::from_secs(2),
    statement_timeout: Duration::from_secs(5),
    application_name: "coral-admin",
};
pub const JOBS_POOL: PoolSettings = PoolSettings {
    max_connections: 2,
    acquire_timeout: Duration::from_secs(30),
    statement_timeout: Duration::from_secs(5),
    application_name: "coral-admin-jobs",
};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub redis: RedisPool,
    pub auth: Arc<AuthConfig>,
    pub discord: DiscordContext,
    pub names: NameService,
    pub views: ViewCache,
    pub coverage: CoverageCache,
    pub jobs: JobRegistry,
    pub rollups: Rollups,
    pub search: SearchLimits,
    pub players: PlayerSearch,
    pub player_lookups: PlayerLookups,
    pub scan_rates: ScanRates,
    pub boot: BootSource,
    pub extensions: Arc<[Arc<dyn Extension>]>,
    pub spikes: SpikeBaselines,
    pub upstream_history: UpstreamHistory,
}

pub struct Connections {
    pub db: Arc<Database>,
    pub jobs_db: Arc<Database>,
    pub redis: RedisPool,
}

impl Connections {
    pub async fn open(database_url: &str, redis: RedisPool) -> Result<Self> {
        let db = Database::connect_with(database_url, INTERACTIVE_POOL)
            .await
            .context("open the interactive database pool")?;
        let jobs_db = Database::connect_with(database_url, JOBS_POOL)
            .await
            .context("open the jobs database pool")?;
        Ok(Self {
            db: Arc::new(db),
            jobs_db: Arc::new(jobs_db),
            redis,
        })
    }
}

impl AppState {
    pub async fn start(
        connections: Connections,
        auth: Arc<AuthConfig>,
        discord: DiscordContext,
    ) -> Result<(Self, NameWorkers)> {
        let Connections {
            db, jobs_db, redis, ..
        } = connections;
        let coverage = CoverageCache::load(db.clone(), redis.clone())
            .await
            .context("read request log coverage")?;
        let (names, workers) = NameService::new(db.clone(), redis.clone(), discord.shared_bot());
        let extensions = extensions::load(&redis);
        let players = PlayerSearch::new(db.clone(), redis.clone());
        let state = Self {
            jobs: JobRegistry::new(jobs_db.clone(), redis.clone()),
            rollups: Rollups::new(jobs_db),
            player_lookups: PlayerLookups::new(db.clone(), players.clone()),
            players,
            scan_rates: ScanRates::new(),
            boot: BootSource::from_env(coverage.clone(), extensions::summaries(&extensions)),
            views: ViewCache::new(),
            search: SearchLimits::new(),
            spikes: SpikeBaselines::new(),
            upstream_history: UpstreamHistory::new(),
            coverage,
            names,
            discord,
            auth,
            extensions,
            db,
            redis,
        };
        Ok((state, workers))
    }
}
