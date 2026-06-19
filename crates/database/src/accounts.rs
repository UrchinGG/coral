use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct MinecraftAccount {
    pub id: i64,
    pub member_id: i64,
    pub uuid: String,
    pub added_at: DateTime<Utc>,
}

pub struct AccountRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> AccountRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn add(&self, member_id: i64, uuid: &str) -> Result<MinecraftAccount, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO minecraft_accounts (member_id, uuid)
             VALUES ($1, $2)
             ON CONFLICT (member_id, uuid) DO UPDATE SET member_id = EXCLUDED.member_id
             RETURNING id, member_id, uuid, added_at",
        )
        .bind(member_id)
        .bind(uuid)
        .fetch_one(self.pool)
        .await
    }

    pub async fn remove(&self, member_id: i64, uuid: &str) -> Result<bool, sqlx::Error> {
        sqlx::query("DELETE FROM minecraft_accounts WHERE member_id = $1 AND uuid = $2")
            .bind(member_id)
            .bind(uuid)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn list(&self, member_id: i64) -> Result<Vec<MinecraftAccount>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, member_id, uuid, added_at
             FROM minecraft_accounts
             WHERE member_id = $1
             ORDER BY added_at",
        )
        .bind(member_id)
        .fetch_all(self.pool)
        .await
    }

    pub async fn list_uuids(&self, discord_id: i64) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT uuid FROM members WHERE discord_id = $1 AND uuid IS NOT NULL
             UNION
             SELECT ma.uuid FROM minecraft_accounts ma
             JOIN members m ON m.id = ma.member_id
             WHERE m.discord_id = $1",
        )
        .bind(discord_id)
        .fetch_all(self.pool)
        .await
    }

    pub async fn is_owned_by(&self, uuid: &str, discord_id: i64) -> Result<bool, sqlx::Error> {
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM members WHERE discord_id = $2 AND uuid = $1
                UNION ALL
                SELECT 1 FROM minecraft_accounts ma
                JOIN members m ON m.id = ma.member_id
                WHERE ma.uuid = $1 AND m.discord_id = $2
            )",
        )
        .bind(uuid)
        .bind(discord_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.is_some_and(|(exists,)| exists))
    }
}
