use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate};
use database::Database;
use reqlog::partitions::{build_partition_index, missing_partition_indexes};
use serde_json::json;
use sqlx::PgConnection;

use crate::error::{AdminError, Context};
use crate::jobs::{Job, JobContext};

pub const INDEX_CATCHUP_JOB: &str = "index-catchup";
pub const INDEX_BUILD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

const INDEX_UNIT: &str = "indexes";

pub struct IndexCatchupJob;

#[async_trait]
impl Job for IndexCatchupJob {
    fn name(&self) -> &'static str {
        INDEX_CATCHUP_JOB
    }

    async fn run(&self, ctx: &JobContext) -> Result<(), AdminError> {
        ctx.wait_for_quiet_window().await?;
        let mut conn = build_connection(ctx.db()).await?;
        let missing = missing_partition_indexes(&mut conn, oldest_partition_day())
            .await
            .context("list partitions missing an index")?;
        for (built, index) in missing.iter().enumerate() {
            ctx.wait_for_quiet_window().await?;
            let name = index.child_name();
            build_partition_index(&mut conn, index)
                .await
                .context(format!("build {name}"))?;
            ctx.report(json!({
                "done": built + 1,
                "total": missing.len(),
                "unit": INDEX_UNIT,
                "last": name,
            }))
            .await?;
        }
        Ok(())
    }
}

pub async fn missing_indexes(db: &Database) -> Result<Vec<String>, AdminError> {
    let mut conn = db.pool().acquire().await.context("acquire a connection")?;
    let missing = missing_partition_indexes(&mut conn, oldest_partition_day())
        .await
        .context("list partitions missing an index")?;
    Ok(missing.iter().map(|index| index.child_name()).collect())
}

fn oldest_partition_day() -> NaiveDate {
    DateTime::UNIX_EPOCH.date_naive()
}

async fn build_connection(db: &Database) -> Result<PgConnection, AdminError> {
    let mut conn = db
        .pool()
        .acquire()
        .await
        .context("acquire a connection for index builds")?
        .detach();
    sqlx::query(&format!(
        "SET statement_timeout = '{}s'",
        INDEX_BUILD_TIMEOUT.as_secs()
    ))
    .execute(&mut conn)
    .await
    .context("allow index builds their own statement timeout")?;
    Ok(conn)
}
