use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct FlagDismissal {
    pub flag_key: String,
    pub dismissed_until: DateTime<Utc>,
}

pub struct FlagDismissalRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> FlagDismissalRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn dismiss(&self, flag_key: &str, until: DateTime<Utc>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO flag_dismissals (flag_key, dismissed_until) VALUES ($1, $2)
             ON CONFLICT (flag_key) DO UPDATE SET dismissed_until = EXCLUDED.dismissed_until",
        )
        .bind(flag_key)
        .bind(until)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_all(&self) -> Result<Vec<FlagDismissal>, sqlx::Error> {
        sqlx::query_as("SELECT flag_key, dismissed_until FROM flag_dismissals")
            .fetch_all(self.pool)
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

    async fn cleanup(pool: &PgPool, flag_key: &str) {
        sqlx::query("DELETE FROM flag_dismissals WHERE flag_key = $1")
            .bind(flag_key)
            .execute(pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn dismiss_then_list_all() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = FlagDismissalRepository::new(&pool);
        let key = format!("test_flag_{}", Utc::now().timestamp_nanos_opt().unwrap());
        cleanup(&pool, &key).await;

        let until = Utc::now() + Duration::hours(24);
        repo.dismiss(&key, until).await.unwrap();

        let all = repo.list_all().await.unwrap();
        let found = all.iter().find(|d| d.flag_key == key).unwrap();
        assert!((found.dismissed_until - until).num_seconds().abs() < 2);

        cleanup(&pool, &key).await;
    }

    #[tokio::test]
    async fn dismiss_overwrites_existing() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = FlagDismissalRepository::new(&pool);
        let key = format!(
            "test_flag_overwrite_{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        );
        cleanup(&pool, &key).await;

        repo.dismiss(&key, Utc::now() + Duration::hours(1))
            .await
            .unwrap();
        let later = Utc::now() + Duration::hours(48);
        repo.dismiss(&key, later).await.unwrap();

        let all = repo.list_all().await.unwrap();
        let matches: Vec<_> = all.iter().filter(|d| d.flag_key == key).collect();
        assert_eq!(matches.len(), 1);
        assert!((matches[0].dismissed_until - later).num_seconds().abs() < 2);

        cleanup(&pool, &key).await;
    }
}
