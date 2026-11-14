use std::collections::HashMap;

use chrono::{DateTime, Utc};
use coral_redis::RedisPool;
use coral_redis::waste::{self, ALLOWANCE, REFILL_PER_DAY};
use serde::Serialize;
use sqlx::PgPool;

use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::serde_ts;

#[derive(Debug, Serialize)]
pub struct WasteOverview {
    allowance: f64,
    refill_per_day: f64,
    keys: Vec<WastingKey>,
}

#[derive(Debug, Serialize)]
pub struct WastingKey {
    account: EntityRef,
    #[serde(flatten)]
    usage: Usage,
}

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

pub async fn overview(
    pool: &PgPool,
    redis: &RedisPool,
    now: DateTime<Utc>,
) -> Result<WasteOverview, AdminError> {
    let mut conn = redis.connection();
    let allowances = waste::allowances(&mut conn, now)
        .await
        .context("read the waste allowances")?;
    let blocks = waste::blocks(&mut conn)
        .await
        .context("read the keys blocked for wasting Hypixel calls")?;
    let mut usage: HashMap<i64, Usage> = HashMap::new();
    for allowance in allowances {
        usage.entry(allowance.member_id).or_default().spent = allowance.spent;
    }
    for block in blocks {
        usage.entry(block.member_id).or_default().blocked_until = Some(block.until());
    }
    let members: Vec<i64> = usage.keys().copied().collect();
    let discord_ids = discord_ids(pool, &members).await?;
    let mut keys: Vec<WastingKey> = usage
        .into_iter()
        .filter_map(|(member_id, usage)| {
            Some(WastingKey {
                account: EntityRef::Account {
                    discord_id: *discord_ids.get(&member_id)?,
                    member_id: Some(member_id),
                },
                usage,
            })
        })
        .collect();
    keys.sort_by(|a, b| {
        let blocked = |key: &WastingKey| key.usage.blocked_until.is_some();
        blocked(b)
            .cmp(&blocked(a))
            .then(b.usage.spent.total_cmp(&a.usage.spent))
    });
    Ok(WasteOverview {
        allowance: ALLOWANCE,
        refill_per_day: REFILL_PER_DAY,
        keys,
    })
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
