use std::time::Duration;

use sqlx::migrate::MigrateError;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgConnection, PgPool};

const NO_STATEMENT_TIMEOUT: &str = "SET statement_timeout = 0";
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

#[derive(Debug, Clone, Copy)]
pub struct PoolSettings {
    pub max_connections: u32,
    pub acquire_timeout: Duration,
    pub statement_timeout: Duration,
    pub application_name: &'static str,
}

impl PoolSettings {
    async fn apply(self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query(
            "SELECT set_config('statement_timeout', $1, false),
                    set_config('jit', 'off', false),
                    set_config('application_name', $2, false)",
        )
        .bind(self.statement_timeout.as_millis().to_string())
        .bind(self.application_name)
        .execute(conn)
        .await?;
        Ok(())
    }
}

impl Database {
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(50)
            .min_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(IDLE_TIMEOUT)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn connect_with(url: &str, settings: PoolSettings) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(settings.max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .idle_timeout(IDLE_TIMEOUT)
            .after_connect(move |conn, _| Box::pin(settings.apply(conn)))
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), MigrateError> {
        // Detach the connection from the pool so it is closed (not returned) when
        // dropped. Postgres releases session-level advisory locks on close, which
        // prevents the migration lock from leaking into idle pool connections.
        let mut conn = self.pool.acquire().await?.detach();
        conn.execute(NO_STATEMENT_TIMEOUT).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: PoolSettings = PoolSettings {
        max_connections: 2,
        acquire_timeout: Duration::from_secs(2),
        statement_timeout: Duration::from_millis(1_500),
        application_name: "coral-admin-test",
    };

    #[tokio::test]
    async fn every_connection_carries_the_pool_settings() {
        dotenvy::dotenv().ok();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let db = Database::connect_with(&url, SETTINGS).await.unwrap();
        let (timeout, jit, name): (String, String, String) = sqlx::query_as(
            "SELECT current_setting('statement_timeout'), current_setting('jit'),
                    current_setting('application_name')",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(timeout, "1500ms");
        assert_eq!(jit, "off");
        assert_eq!(name, "coral-admin-test");
    }

    #[tokio::test]
    async fn statement_timeout_cancels_slow_statements() {
        dotenvy::dotenv().ok();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let db = Database::connect_with(&url, SETTINGS).await.unwrap();
        let error = sqlx::query("SELECT pg_sleep(3)")
            .execute(db.pool())
            .await
            .unwrap_err();
        let code = error.as_database_error().and_then(|e| e.code());
        assert_eq!(code.as_deref(), Some("57014"));
    }
}
