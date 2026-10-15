use std::sync::Arc;
use std::time::Duration;

use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::{AdminError, Context};

pub const SEARCH_PROBE_CONCURRENCY: usize = 6;
const SMALLEST_BOUND_MS: u128 = 1;

#[derive(Clone)]
pub struct SearchLimits {
    probes: Arc<Semaphore>,
}

impl SearchLimits {
    pub fn new() -> Self {
        Self {
            probes: Arc::new(Semaphore::new(SEARCH_PROBE_CONCURRENCY)),
        }
    }

    pub async fn probe(&self) -> OwnedSemaphorePermit {
        self.probes
            .clone()
            .acquire_owned()
            .await
            .expect("the search probe semaphore is never closed")
    }
}

pub async fn begin_bounded(
    pool: &PgPool,
    timeout: Duration,
) -> Result<Transaction<'static, Postgres>, AdminError> {
    let mut tx = pool.begin().await.context("begin a bounded query")?;
    bound_statements(&mut tx, timeout).await?;
    Ok(tx)
}

pub async fn acquire_within(
    pool: &PgPool,
    budget: Duration,
) -> Result<PoolConnection<Postgres>, AdminError> {
    tokio::time::timeout(budget, pool.acquire())
        .await
        .map_err(|_| {
            AdminError::Timeout(format!(
                "no database connection became free within {} ms",
                budget.as_millis()
            ))
        })?
        .context("acquire a database connection")
}

pub async fn bound_statements(
    conn: &mut PgConnection,
    timeout: Duration,
) -> Result<(), AdminError> {
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(timeout.as_millis().max(SMALLEST_BOUND_MS).to_string())
        .execute(conn)
        .await
        .context("bound the query's statement timeout")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounded_transactions_cancel_their_statements_server_side() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url).await.unwrap();
        let mut tx = begin_bounded(&pool, Duration::from_millis(50))
            .await
            .unwrap();
        let error = sqlx::query("SELECT pg_sleep(1)")
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(AdminError::from(error).code(), "timeout");
    }

    #[tokio::test]
    async fn the_bound_ends_with_the_transaction() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let before: String = sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        drop(conn);
        let mut tx = begin_bounded(&pool, Duration::from_millis(250))
            .await
            .unwrap();
        let inside: String = sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let after: String = sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(inside, "250ms");
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn probes_are_capped() {
        let limits = SearchLimits::new();
        let mut permits = Vec::new();
        for _ in 0..SEARCH_PROBE_CONCURRENCY {
            permits.push(limits.probe().await);
        }
        assert_eq!(limits.probes.available_permits(), 0);
        drop(permits);
        assert_eq!(limits.probes.available_permits(), SEARCH_PROBE_CONCURRENCY);
    }
}
