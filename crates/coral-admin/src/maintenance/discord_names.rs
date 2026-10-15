use std::time::Duration;

use async_trait::async_trait;
use database::Database;
use serde_json::json;

use crate::error::{AdminError, Context};
use crate::jobs::{Job, JobContext};
use crate::names::NameService;

pub const DISCORD_NAMES_JOB: &str = "discord-names";
pub const DISCORD_NAMES_BATCH: usize = 20;
pub const QUEUE_DRAIN_POLL: Duration = Duration::from_secs(1);

const ID_UNIT: &str = "ids";

pub struct DiscordNamesJob {
    names: NameService,
    ids: Vec<i64>,
}

impl DiscordNamesJob {
    pub async fn planned(db: &Database, names: NameService) -> Result<Self, AdminError> {
        if !names.fetches_discord() {
            return Err(AdminError::Rejected(
                "DISCORD_TOKEN is not configured, so Discord names cannot be fetched".into(),
            ));
        }
        let ids = missing_ids(db).await?;
        if ids.is_empty() {
            return Err(AdminError::Rejected(
                "every member's Discord name is cached".into(),
            ));
        }
        Ok(Self { names, ids })
    }
}

#[async_trait]
impl Job for DiscordNamesJob {
    fn name(&self) -> &'static str {
        DISCORD_NAMES_JOB
    }

    async fn run(&self, ctx: &JobContext) -> Result<(), AdminError> {
        let mut done = 0;
        let mut cached = 0;
        for batch in self.ids.chunks(DISCORD_NAMES_BATCH) {
            ctx.wait_for_quiet_window().await?;
            self.names.queue_discord(batch);
            while self.names.discord_queued(batch) > 0 {
                tokio::time::sleep(QUEUE_DRAIN_POLL).await;
            }
            done += batch.len();
            cached += cached_among(ctx.db(), batch).await?;
            ctx.report(json!({
                "done": done,
                "total": self.ids.len(),
                "unit": ID_UNIT,
                "cached": cached,
            }))
            .await?;
        }
        Ok(())
    }
}

pub async fn missing_discord_names(db: &Database) -> Result<i64, AdminError> {
    sqlx::query_scalar(
        "SELECT count(*) FROM members m
         WHERE NOT EXISTS (SELECT 1 FROM discord_username_cache c WHERE c.discord_id = m.discord_id)",
    )
    .fetch_one(db.pool())
    .await
    .context("count members without a cached Discord name")
}

async fn missing_ids(db: &Database) -> Result<Vec<i64>, AdminError> {
    sqlx::query_scalar(
        "SELECT m.discord_id FROM members m
         WHERE NOT EXISTS (SELECT 1 FROM discord_username_cache c WHERE c.discord_id = m.discord_id)
         ORDER BY m.discord_id",
    )
    .fetch_all(db.pool())
    .await
    .context("list members without a cached Discord name")
}

async fn cached_among(db: &Database, ids: &[i64]) -> Result<usize, AdminError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM discord_username_cache WHERE discord_id = ANY($1)",
    )
    .bind(ids)
    .fetch_one(db.pool())
    .await
    .context("count the Discord names cached so far")?;
    Ok(usize::try_from(count).unwrap_or_default())
}
