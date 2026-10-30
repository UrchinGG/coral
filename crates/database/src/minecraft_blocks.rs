use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct MinecraftBlock {
    pub uuid: Uuid,
    pub reason: Option<String>,
    pub blocked_by: i64,
    pub blocked_at: DateTime<Utc>,
}

pub struct MinecraftBlockRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> MinecraftBlockRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, uuid: Uuid) -> Result<Option<MinecraftBlock>, sqlx::Error> {
        sqlx::query_as(
            "SELECT uuid, reason, blocked_by, blocked_at FROM minecraft_blocks WHERE uuid = $1",
        )
        .bind(uuid)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn block(
        &self,
        uuid: Uuid,
        reason: Option<&str>,
        blocked_by: i64,
    ) -> Result<bool, sqlx::Error> {
        let inserted = sqlx::query(
            "INSERT INTO minecraft_blocks (uuid, reason, blocked_by) VALUES ($1, $2, $3)
             ON CONFLICT (uuid) DO NOTHING",
        )
        .bind(uuid)
        .bind(reason)
        .bind(blocked_by)
        .execute(self.pool)
        .await?;
        Ok(inserted.rows_affected() == 1)
    }

    pub async fn unblock(&self, uuid: Uuid) -> Result<bool, sqlx::Error> {
        let deleted = sqlx::query("DELETE FROM minecraft_blocks WHERE uuid = $1")
            .bind(uuid)
            .execute(self.pool)
            .await?;
        Ok(deleted.rows_affected() == 1)
    }
}
