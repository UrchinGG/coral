use chrono::{Days, NaiveDate, Utc};
use sqlx::{Executor, FromRow, PgPool, raw_sql};
use tokio::sync::watch;

use crate::{
    CREATION_RETRY, DDL_LOCK_TIMEOUT, DETAIL_RETENTION_DAYS, DROP_LOCK_TIMEOUT, LOG_RETENTION_DAYS,
    MAINTENANCE_INTERVAL, PARTITION_LOCK_KEY, PARTITIONS_AHEAD_DAYS, SUBJECT_RETENTION_DAYS,
};

pub const PARTITIONED_TABLES: [(&str, u64); 3] = [
    ("api_request_log", LOG_RETENTION_DAYS),
    ("api_request_detail", DETAIL_RETENTION_DAYS),
    ("api_request_subject", SUBJECT_RETENTION_DAYS),
];

#[derive(Debug, FromRow)]
struct MissingIndex {
    parent_index: String,
    partition: String,
    access_method: String,
    is_unique: bool,
}

pub fn spawn_maintenance(pool: PgPool) -> watch::Receiver<bool> {
    let (ready, receiver) = watch::channel(false);
    tokio::spawn(async move {
        create_until_ready(&pool).await;
        ready.send_replace(true);
        spawn_index_catch_up(pool.clone());
        loop {
            drop_expired(&pool).await;
            tokio::time::sleep(MAINTENANCE_INTERVAL).await;
            if let Err(e) = create_upcoming(&pool).await {
                tracing::warn!(error = %e, "partition creation failed; retrying next tick");
            }
        }
    });
    receiver
}

pub async fn create_upcoming(pool: &PgPool) -> Result<(), sqlx::Error> {
    let today = Utc::now().date_naive();
    for (table, _) in PARTITIONED_TABLES {
        for offset in 0..=PARTITIONS_AHEAD_DAYS {
            create_partition(pool, table, today + Days::new(offset)).await?;
        }
    }
    Ok(())
}

pub async fn drop_expired(pool: &PgPool) {
    let today = Utc::now().date_naive();
    for (table, retention_days) in PARTITIONED_TABLES {
        let cutoff = today - Days::new(retention_days);
        let expired = match partitions_before(pool, table, cutoff).await {
            Ok(expired) => expired,
            Err(e) => {
                tracing::warn!(table, error = %e, "expired partitions not listed");
                continue;
            }
        };
        for partition in expired {
            if let Err(e) = drop_partition(pool, &partition).await {
                tracing::warn!(partition, error = %e, "partition drop failed; retrying next tick");
            }
        }
    }
}

pub async fn ensure_partition_indexes(pool: &PgPool, since: NaiveDate) -> Result<(), sqlx::Error> {
    for (table, _) in PARTITIONED_TABLES {
        for missing in missing_indexes(pool, table, since).await? {
            build_child_index(pool, &missing).await?;
        }
    }
    Ok(())
}

async fn create_until_ready(pool: &PgPool) {
    while let Err(e) = create_upcoming(pool).await {
        tracing::warn!(error = %e, "partition creation failed; request log inserts are waiting");
        tokio::time::sleep(CREATION_RETRY).await;
    }
}

fn spawn_index_catch_up(pool: PgPool) {
    tokio::spawn(async move {
        if let Err(e) = ensure_partition_indexes(&pool, Utc::now().date_naive()).await {
            tracing::warn!(error = %e, "partition index catch-up failed");
        }
    });
}

async fn create_partition(pool: &PgPool, table: &str, day: NaiveDate) -> Result<(), sqlx::Error> {
    let partition = partition_name(table, day);
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(PARTITION_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    if is_attached(&mut tx, table, &partition).await? {
        return Ok(());
    }
    let from = day.format("%Y-%m-%d 00:00:00+00");
    let to = (day + Days::new(1)).format("%Y-%m-%d 00:00:00+00");
    let attach = format!(
        "SET LOCAL lock_timeout = '{DDL_LOCK_TIMEOUT}';
         CREATE TABLE IF NOT EXISTS {partition} (LIKE {table} INCLUDING DEFAULTS INCLUDING CONSTRAINTS);
         ALTER TABLE {table} ATTACH PARTITION {partition} FOR VALUES FROM ('{from}') TO ('{to}');"
    );
    tx.execute(raw_sql(&attach)).await?;
    tx.commit().await?;
    tracing::info!(partition, "partition attached");
    Ok(())
}

async fn is_attached(
    conn: &mut sqlx::PgConnection,
    table: &str,
    partition: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
           SELECT 1 FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
           WHERE i.inhparent = $1::regclass AND c.relname = $2)",
    )
    .bind(table)
    .bind(partition)
    .fetch_one(conn)
    .await
}

async fn partitions_before(
    pool: &PgPool,
    table: &str,
    cutoff: NaiveDate,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT c.relname::text FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
         WHERE i.inhparent = $1::regclass AND c.relname ~ $2
           AND to_date(right(c.relname, 8), 'YYYYMMDD') < $3
         ORDER BY c.relname",
    )
    .bind(table)
    .bind(partition_pattern(table))
    .bind(cutoff)
    .fetch_all(pool)
    .await
}

async fn drop_partition(pool: &PgPool, partition: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let drop = format!(
        "SET LOCAL lock_timeout = '{DROP_LOCK_TIMEOUT}'; DROP TABLE IF EXISTS {partition};"
    );
    tx.execute(raw_sql(&drop)).await?;
    tx.commit().await?;
    tracing::info!(partition, "expired partition dropped");
    Ok(())
}

async fn missing_indexes(
    pool: &PgPool,
    table: &str,
    since: NaiveDate,
) -> Result<Vec<MissingIndex>, sqlx::Error> {
    sqlx::query_as(
        "SELECT parent.relname::text AS parent_index, part.relname::text AS partition,
                substring(pg_get_indexdef(ix.indexrelid) from ' USING .*$') AS access_method,
                ix.indisunique AS is_unique
         FROM pg_index ix
         JOIN pg_class parent ON parent.oid = ix.indexrelid
         JOIN pg_inherits inh ON inh.inhparent = ix.indrelid
         JOIN pg_class part ON part.oid = inh.inhrelid
         WHERE ix.indrelid = $1::regclass
           AND part.relname ~ $2
           AND to_date(right(part.relname, 8), 'YYYYMMDD') >= $3
           AND NOT EXISTS (
             SELECT 1 FROM pg_inherits child
             JOIN pg_index cx ON cx.indexrelid = child.inhrelid
             WHERE child.inhparent = ix.indexrelid AND cx.indrelid = part.oid)
         ORDER BY part.relname, parent.relname",
    )
    .bind(table)
    .bind(partition_pattern(table))
    .bind(since)
    .fetch_all(pool)
    .await
}

async fn build_child_index(pool: &PgPool, missing: &MissingIndex) -> Result<(), sqlx::Error> {
    let child = format!("{}_{}", missing.partition, missing.parent_index);
    let valid: Option<bool> =
        sqlx::query_scalar("SELECT indisvalid FROM pg_index WHERE indexrelid = to_regclass($1)")
            .bind(&child)
            .fetch_optional(pool)
            .await?;
    if valid == Some(false) {
        let discard = format!("DROP INDEX CONCURRENTLY IF EXISTS {child}");
        pool.execute(raw_sql(&discard)).await?;
    }
    let unique = if missing.is_unique { "UNIQUE " } else { "" };
    let build = format!(
        "CREATE {unique}INDEX CONCURRENTLY IF NOT EXISTS {child} ON {}{}",
        missing.partition, missing.access_method
    );
    pool.execute(raw_sql(&build)).await?;
    let mut tx = pool.begin().await?;
    let attach = format!(
        "SET LOCAL lock_timeout = '{DDL_LOCK_TIMEOUT}';
         ALTER INDEX {} ATTACH PARTITION {child};",
        missing.parent_index
    );
    tx.execute(raw_sql(&attach)).await?;
    tx.commit().await?;
    tracing::info!(index = child, "partition index attached");
    Ok(())
}

fn partition_name(table: &str, day: NaiveDate) -> String {
    format!("{table}_{}", day.format("%Y%m%d"))
}

fn partition_pattern(table: &str) -> String {
    format!("^{table}_[0-9]{{8}}$")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitions_are_named_by_utc_day() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        assert_eq!(
            partition_name("api_request_subject", day),
            "api_request_subject_20261014"
        );
        assert_eq!(
            partition_pattern("api_request_log"),
            "^api_request_log_[0-9]{8}$"
        );
    }

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        PgPool::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()
    }

    #[tokio::test]
    async fn creation_is_idempotent_and_attaches_every_table() {
        let Some(pool) = test_pool().await else {
            return;
        };
        create_upcoming(&pool).await.unwrap();
        create_upcoming(&pool).await.unwrap();
        let today = Utc::now().date_naive();
        for (table, _) in PARTITIONED_TABLES {
            for offset in 0..=PARTITIONS_AHEAD_DAYS {
                let partition = partition_name(table, today + Days::new(offset));
                let mut conn = pool.acquire().await.unwrap();
                assert!(is_attached(&mut conn, table, &partition).await.unwrap());
            }
        }
    }

    #[tokio::test]
    async fn catch_up_builds_and_attaches_missing_child_indexes() {
        let Some(pool) = test_pool().await else {
            return;
        };
        create_upcoming(&pool).await.unwrap();
        let today = Utc::now().date_naive();
        ensure_partition_indexes(&pool, today).await.unwrap();
        for (table, _) in PARTITIONED_TABLES {
            assert!(
                missing_indexes(&pool, table, today)
                    .await
                    .unwrap()
                    .is_empty(),
                "{table}"
            );
        }
    }
}
