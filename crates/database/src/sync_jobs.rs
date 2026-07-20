use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct GuildSyncJob {
    pub id: i64,
    pub guild_id: i64,
    pub kind: String,
    pub payload: Value,
    pub status: String,
    pub processed: i32,
    pub total: Option<i32>,
    pub cancel_requested: bool,
    pub error: Option<String>,
    pub created_by: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

const JOB_COLUMNS: &str = "id, guild_id, kind, payload, status, processed, total,
    cancel_requested, error, created_by, created_at, started_at, finished_at";

pub struct GuildSyncJobRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> GuildSyncJobRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(
        &self,
        guild_id: i64,
        kind: &str,
        payload: &Value,
        created_by: Option<i64>,
    ) -> Result<GuildSyncJob, sqlx::Error> {
        sqlx::query_as(&format!(
            "INSERT INTO guild_sync_jobs (guild_id, kind, payload, created_by)
             VALUES ($1, $2, $3, $4)
             RETURNING {JOB_COLUMNS}"
        ))
        .bind(guild_id)
        .bind(kind)
        .bind(payload)
        .bind(created_by)
        .fetch_one(self.pool)
        .await
    }

    pub async fn get(&self, id: i64) -> Result<Option<GuildSyncJob>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {JOB_COLUMNS} FROM guild_sync_jobs WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn claim(&self, id: i64) -> Result<Option<GuildSyncJob>, sqlx::Error> {
        sqlx::query_as(&format!(
            "UPDATE guild_sync_jobs SET status = 'running', started_at = NOW()
             WHERE id = $1 AND status = 'queued'
               AND NOT EXISTS (
                   SELECT 1 FROM guild_sync_jobs other
                   WHERE other.guild_id = guild_sync_jobs.guild_id
                     AND other.status = 'running'
                     AND other.id != guild_sync_jobs.id
               )
             RETURNING {JOB_COLUMNS}"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn update_progress(
        &self,
        id: i64,
        processed: i32,
        total: i32,
    ) -> Result<bool, sqlx::Error> {
        let cancel_requested: Option<(bool,)> = sqlx::query_as(
            "UPDATE guild_sync_jobs SET processed = $2, total = $3
             WHERE id = $1 RETURNING cancel_requested",
        )
        .bind(id)
        .bind(processed)
        .bind(total)
        .fetch_optional(self.pool)
        .await?;
        Ok(cancel_requested.is_some_and(|(c,)| c))
    }

    pub async fn request_cancel(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE guild_sync_jobs SET cancel_requested = true
             WHERE id = $1 AND status IN ('queued', 'running')",
        )
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn finish(
        &self,
        id: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE guild_sync_jobs SET status = $2, error = $3, finished_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(error)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn next_queued_for_guild(&self, guild_id: i64) -> Result<Option<i64>, sqlx::Error> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM guild_sync_jobs
             WHERE guild_id = $1 AND status = 'queued'
             ORDER BY id LIMIT 1",
        )
        .bind(guild_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|(id,)| id))
    }

    pub async fn reset_running_to_queued(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE guild_sync_jobs SET status = 'queued', started_at = NULL
             WHERE status = 'running'",
        )
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_queued(&self) -> Result<Vec<GuildSyncJob>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {JOB_COLUMNS} FROM guild_sync_jobs WHERE status = 'queued' ORDER BY id"
        ))
        .fetch_all(self.pool)
        .await
    }

    pub async fn list_recent(
        &self,
        guild_id: i64,
        finished_limit: i64,
    ) -> Result<Vec<GuildSyncJob>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {JOB_COLUMNS} FROM (
                 SELECT * FROM guild_sync_jobs
                 WHERE guild_id = $1 AND status IN ('queued', 'running')
                 UNION ALL
                 (SELECT * FROM guild_sync_jobs
                  WHERE guild_id = $1 AND status IN ('done', 'cancelled', 'failed')
                  ORDER BY id DESC LIMIT $2)
             ) jobs ORDER BY id DESC"
        ))
        .bind(guild_id)
        .bind(finished_limit)
        .fetch_all(self.pool)
        .await
    }
}
