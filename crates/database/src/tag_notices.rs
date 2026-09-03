use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct PendingTagNotice {
    pub id: i64,
    pub discord_id: i64,
    pub uuid: String,
    pub username: String,
    pub tag_type: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

pub struct TagNoticeRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> TagNoticeRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn queue(
        &self,
        discord_id: i64,
        uuid: &str,
        username: &str,
        tag_type: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO pending_tag_notices (discord_id, uuid, username, tag_type, reason)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(discord_id)
        .bind(uuid)
        .bind(username)
        .bind(tag_type)
        .bind(reason)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_pending(
        &self,
        discord_id: i64,
    ) -> Result<Vec<PendingTagNotice>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, discord_id, uuid, username, tag_type, reason, created_at
             FROM pending_tag_notices
             WHERE discord_id = $1 AND delivered_at IS NULL
             ORDER BY created_at",
        )
        .bind(discord_id)
        .fetch_all(self.pool)
        .await
    }

    /// Marks notices as delivered so they are never shown a second time.
    pub async fn mark_delivered(&self, ids: &[i64]) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE pending_tag_notices SET delivered_at = now() WHERE id = ANY($1)")
            .bind(ids)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn has_pending(&self, discord_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM pending_tag_notices
                WHERE discord_id = $1 AND delivered_at IS NULL
            )",
        )
        .bind(discord_id)
        .fetch_one(self.pool)
        .await
    }
}
