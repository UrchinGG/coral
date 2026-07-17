use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct CachedDiscordUsername {
    pub discord_id: i64,
    pub username: String,
    pub last_refreshed: DateTime<Utc>,
}

pub struct DiscordUsernameCacheRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> DiscordUsernameCacheRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, discord_id: i64) -> Result<Option<CachedDiscordUsername>, sqlx::Error> {
        sqlx::query_as(
            "SELECT discord_id, username, last_refreshed
             FROM discord_username_cache WHERE discord_id = $1",
        )
        .bind(discord_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn get_many(
        &self,
        discord_ids: &[i64],
    ) -> Result<HashMap<i64, CachedDiscordUsername>, sqlx::Error> {
        let rows: Vec<CachedDiscordUsername> = sqlx::query_as(
            "SELECT discord_id, username, last_refreshed
             FROM discord_username_cache WHERE discord_id = ANY($1)",
        )
        .bind(discord_ids)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| (r.discord_id, r)).collect())
    }

    pub async fn upsert(
        &self,
        discord_id: i64,
        username: &str,
    ) -> Result<CachedDiscordUsername, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO discord_username_cache (discord_id, username, last_refreshed)
             VALUES ($1, $2, NOW())
             ON CONFLICT (discord_id) DO UPDATE SET
                username = EXCLUDED.username,
                last_refreshed = NOW()
             RETURNING discord_id, username, last_refreshed",
        )
        .bind(discord_id)
        .bind(username)
        .fetch_one(self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()
    }

    fn test_discord_id(seed: i64) -> i64 {
        910_000_000_000_000_000 + seed
    }

    async fn cleanup(pool: &PgPool, discord_id: i64) {
        sqlx::query("DELETE FROM discord_username_cache WHERE discord_id = $1")
            .bind(discord_id)
            .execute(pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn upsert_then_get() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = DiscordUsernameCacheRepository::new(&pool);
        let id = test_discord_id(1);
        cleanup(&pool, id).await;

        repo.upsert(id, "aiden").await.unwrap();
        let found = repo.get(id).await.unwrap().unwrap();
        assert_eq!(found.username, "aiden");

        repo.upsert(id, "aiden_renamed").await.unwrap();
        let updated = repo.get(id).await.unwrap().unwrap();
        assert_eq!(updated.username, "aiden_renamed");
        assert!(updated.last_refreshed >= found.last_refreshed);

        cleanup(&pool, id).await;
    }

    #[tokio::test]
    async fn get_many_returns_only_cached_ids() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = DiscordUsernameCacheRepository::new(&pool);
        let a = test_discord_id(2);
        let b = test_discord_id(3);
        let missing = test_discord_id(4);
        cleanup(&pool, a).await;
        cleanup(&pool, b).await;

        repo.upsert(a, "user_a").await.unwrap();
        repo.upsert(b, "user_b").await.unwrap();

        let found = repo.get_many(&[a, b, missing]).await.unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[&a].username, "user_a");
        assert_eq!(found[&b].username, "user_b");
        assert!(!found.contains_key(&missing));

        cleanup(&pool, a).await;
        cleanup(&pool, b).await;
    }

    #[tokio::test]
    async fn last_refreshed_reflects_staleness_window() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = DiscordUsernameCacheRepository::new(&pool);
        let id = test_discord_id(5);
        cleanup(&pool, id).await;

        let entry = repo.upsert(id, "fresh").await.unwrap();
        assert!(Utc::now() - entry.last_refreshed < Duration::seconds(5));

        cleanup(&pool, id).await;
    }
}
