use std::collections::HashMap;

use chrono::{DateTime, Utc};
use coral_redis::RedisPool;
use coral_redis::waste::{self, ALLOWANCE, REFILL_PER_DAY};
use serde::Serialize;
use sqlx::PgPool;

use crate::error::{AdminError, Context};
use crate::serde_ts;

#[derive(Debug, Serialize)]
pub struct WasteStanding {
    allowance: f64,
    refill_per_day: f64,
    #[serde(flatten)]
    usage: Usage,
}

#[derive(Debug, Default, Serialize)]
pub struct Usage {
    spent: f64,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    blocked_until: Option<DateTime<Utc>>,
}

pub async fn standing(
    redis: &RedisPool,
    member_id: Option<i64>,
    now: DateTime<Utc>,
) -> Result<WasteStanding, AdminError> {
    let usage = match member_id {
        Some(member_id) => usage_of(redis, member_id, now).await?,
        None => Usage::default(),
    };
    Ok(WasteStanding {
        allowance: ALLOWANCE,
        refill_per_day: REFILL_PER_DAY,
        usage,
    })
}

pub async fn discord_ids(pool: &PgPool, members: &[i64]) -> Result<HashMap<i64, i64>, AdminError> {
    let rows: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, discord_id FROM members WHERE id = ANY($1)")
            .bind(members)
            .fetch_all(pool)
            .await
            .context("find the accounts of keys spending their waste allowance")?;
    Ok(rows.into_iter().collect())
}

async fn usage_of(
    redis: &RedisPool,
    member_id: i64,
    now: DateTime<Utc>,
) -> Result<Usage, AdminError> {
    let mut conn = redis.connection();
    let spent = waste::spent(&mut conn, member_id, now)
        .await
        .context("read the key's waste allowance")?;
    let block = waste::block(&mut conn, member_id)
        .await
        .context("read the key's waste block")?;
    Ok(Usage {
        spent,
        blocked_until: block.map(|block| block.until()),
    })
}
