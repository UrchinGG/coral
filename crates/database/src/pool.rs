use std::time::Duration;

use sqlx::PgPool;
use sqlx::migrate::MigrateError;
use sqlx::postgres::PgPoolOptions;

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(50)
            .min_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(600))
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), MigrateError> {
        // Detach the connection from the pool so it is closed (not returned) when
        // dropped. Postgres releases session-level advisory locks on close, which
        // prevents the migration lock from leaking into idle pool connections.
        let mut conn = self.pool.acquire().await?.detach();
        sqlx::migrate!("../../migrations").run(&mut conn).await
    }

    pub async fn migrate_tolerating_newer_schema(&self) -> Result<(), MigrateError> {
        match self.migrate().await {
            Err(MigrateError::VersionMissing(version)) => {
                tracing::warn!(
                    version,
                    "database has a migration this build does not include; continuing"
                );
                Ok(())
            }
            result => result,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
