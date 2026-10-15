use chrono::{Days, NaiveDate};
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::PARTITION_LOCK_KEY;
use crate::partitions::PARTITIONED_TABLES;

pub static NETWORK_STATE: Mutex<()> = Mutex::const_new(());

pub async fn test_pool() -> Option<PgPool> {
    dotenvy::dotenv().ok();
    PgPool::connect(&std::env::var("DATABASE_URL").ok()?)
        .await
        .ok()
}

pub async fn create_day_partitions(pool: &PgPool, day: NaiveDate) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(PARTITION_LOCK_KEY)
        .execute(&mut *tx)
        .await
        .unwrap();
    for (table, _) in PARTITIONED_TABLES {
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {table}_{day} PARTITION OF {table}
             FOR VALUES FROM ('{from}') TO ('{to}')",
            day = day.format("%Y%m%d"),
            from = day,
            to = day + Days::new(1),
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}
