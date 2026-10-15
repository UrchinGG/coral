use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use database::Database;
use sqlx::PgPool;
use tokio::sync::OnceCell;
use uuid::Uuid;

use super::AccountTarget;

const FIXTURE_DAY: (i32, u32, u32) = (2020, 6, 1);
const PARTITIONED: [&str; 2] = ["api_request_log", "api_request_subject"];
const SNOWFLAKE_BASE: i64 = 500_000_000_000_000_000;
const SNOWFLAKE_SPAN: u128 = 100_000_000_000_000_000;

static PARTITIONS: OnceCell<()> = OnceCell::const_new();

pub async fn pool() -> Option<PgPool> {
    let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
        .await
        .ok()?;
    PARTITIONS
        .get_or_init(|| create_partitions(db.pool()))
        .await;
    Some(db.pool().clone())
}

pub fn at(minute: i64) -> DateTime<Utc> {
    let (year, month, day) = FIXTURE_DAY;
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap() + TimeDelta::minutes(minute)
}

pub fn snowflake() -> i64 {
    SNOWFLAKE_BASE + (Uuid::new_v4().as_u128() % SNOWFLAKE_SPAN) as i64
}

pub fn unique_key() -> String {
    Uuid::new_v4().to_string()
}

pub async fn member(pool: &PgPool, api_key: &str) -> AccountTarget {
    let discord_id = snowflake();
    let member_id = sqlx::query_scalar(
        "INSERT INTO members (discord_id, api_key) VALUES ($1, $2) RETURNING id",
    )
    .bind(discord_id)
    .bind(api_key)
    .fetch_one(pool)
    .await
    .unwrap();
    AccountTarget {
        discord_id,
        member_id: Some(member_id),
    }
}

async fn create_partitions(pool: &PgPool) {
    let today = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    for from in [at(0), today] {
        for table in PARTITIONED {
            create_partition(pool, table, from).await;
        }
    }
}

async fn create_partition(pool: &PgPool, table: &str, from: DateTime<Utc>) {
    let to = from + TimeDelta::days(1);
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {table}_{} PARTITION OF {table} FOR VALUES FROM ('{from}') TO ('{to}')",
        from.format("%Y%m%d")
    ))
    .execute(pool)
    .await
    .unwrap();
}
