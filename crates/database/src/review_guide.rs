use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct ReviewGuideConfig {
    pub content: Value,
    pub review_ping_role_id: Option<i64>,
    pub dispute_ping_role_id: Option<i64>,
    pub posted_channel_id: Option<i64>,
    pub posted_thread_id: Option<i64>,
    pub posted_message_id: Option<i64>,
    pub posted_at: Option<DateTime<Utc>>,
    pub posted_by: Option<i64>,
    pub content_updated_at: DateTime<Utc>,
}

const GUIDE_COLUMNS: &str = "content, review_ping_role_id, dispute_ping_role_id,
    posted_channel_id, posted_thread_id, posted_message_id, posted_at, posted_by,
    content_updated_at";

pub struct ReviewGuideRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> ReviewGuideRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self) -> Result<Option<ReviewGuideConfig>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {GUIDE_COLUMNS} FROM review_guide_config WHERE id = 1"
        ))
        .fetch_optional(self.pool)
        .await
    }

    pub async fn get_ping_roles(&self) -> Result<(Option<i64>, Option<i64>), sqlx::Error> {
        let row: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT review_ping_role_id, dispute_ping_role_id
             FROM review_guide_config WHERE id = 1",
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row.unwrap_or((None, None)))
    }

    pub async fn update_content(&self, content: &Value) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE review_guide_config
             SET content = $1, content_updated_at = NOW() WHERE id = 1",
        )
        .bind(content)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_ping_roles(
        &self,
        review_role_id: Option<i64>,
        dispute_role_id: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE review_guide_config
             SET review_ping_role_id = $1, dispute_ping_role_id = $2 WHERE id = 1",
        )
        .bind(review_role_id)
        .bind(dispute_role_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_posted(
        &self,
        channel_id: i64,
        thread_id: i64,
        message_id: i64,
        posted_by: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE review_guide_config
             SET posted_channel_id = $1, posted_thread_id = $2, posted_message_id = $3,
                 posted_at = NOW(), posted_by = $4
             WHERE id = 1",
        )
        .bind(channel_id)
        .bind(thread_id)
        .bind(message_id)
        .bind(posted_by)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}
