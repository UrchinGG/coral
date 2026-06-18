use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct GuildSubscription {
    pub guild_id: String,
    pub discord_id: i64,
    pub tag_types: Vec<String>,
}

pub struct GuildSubscriptionRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> GuildSubscriptionRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_for_user(
        &self,
        discord_id: i64,
    ) -> Result<Option<GuildSubscription>, sqlx::Error> {
        sqlx::query_as(
            "SELECT guild_id, discord_id, tag_types FROM guild_subscriptions
             WHERE discord_id = $1 LIMIT 1",
        )
        .bind(discord_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn set_for_user(
        &self,
        discord_id: i64,
        guild_id: &str,
        tag_types: &[String],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM guild_subscriptions WHERE discord_id = $1")
            .bind(discord_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO guild_subscriptions (guild_id, discord_id, tag_types) VALUES ($1, $2, $3)",
        )
        .bind(guild_id)
        .bind(discord_id)
        .bind(tag_types)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    pub async fn set_tags(&self, discord_id: i64, tag_types: &[String]) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE guild_subscriptions SET tag_types = $2 WHERE discord_id = $1")
            .bind(discord_id)
            .bind(tag_types)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn clear_for_user(&self, discord_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM guild_subscriptions WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn subscribers_for(
        &self,
        guild_id: &str,
    ) -> Result<Vec<GuildSubscription>, sqlx::Error> {
        sqlx::query_as(
            "SELECT guild_id, discord_id, tag_types FROM guild_subscriptions WHERE guild_id = $1",
        )
        .bind(guild_id)
        .fetch_all(self.pool)
        .await
    }
}
