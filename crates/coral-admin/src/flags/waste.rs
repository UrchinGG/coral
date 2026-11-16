use chrono::{DateTime, TimeDelta, Utc};
use clients::FlagLevel;
use coral_redis::RedisPool;
use coral_redis::waste::{self, ALLOWANCE, Block, REFILL_PER_DAY};
use futures_util::future::try_join_all;
use reqlog::KeyKind;
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use super::{Flag, FlagKind, dismissed, grouped};
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::serde_ts;
use crate::waste::discord_ids;

const UNTIL_FORMAT: &str = "%b %-d, %H:%M UTC";
const LOOKBACK: TimeDelta = TimeDelta::hours(24);
const BUCKET: TimeDelta = TimeDelta::minutes(15);

const SUMMARY_SQL: &str = "SELECT count(*) AS lookups,
        count(DISTINCT coalesce(player::text, lower(name))) AS players,
        (SELECT user_agent FROM api_member_agents
          WHERE member_id = $1 AND first_seen <= $3 ORDER BY last_seen DESC LIMIT 1) AS client
     FROM api_request_subject
     WHERE member_id = $1 AND key_kind = $4 AND ts >= $2 AND ts < $3";

const BUCKETS_SQL: &str =
    "SELECT floor(extract(epoch FROM ts - $2) / $5)::int8 AS bucket, count(*) AS lookups
     FROM api_request_subject
     WHERE member_id = $1 AND key_kind = $4 AND ts >= $2 AND ts < $3
     GROUP BY 1";

/// Personal keys blocked for wasting Hypixel calls, with what their lookups looked like in the
/// day before the block. Holds no keys or IPs, so staff below the owner may read it.
#[derive(Serialize)]
pub struct BlockedKeys {
    items: Vec<BlockedKey>,
    step_s: i64,
}

#[derive(Serialize)]
struct BlockedKey {
    flag_key: String,
    account: EntityRef,
    #[serde(serialize_with = "serde_ts::micros")]
    blocked_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    until: DateTime<Utc>,
    #[serde(flatten)]
    summary: LookupSummary,
    series: Vec<i64>,
}

#[derive(Serialize, FromRow)]
struct LookupSummary {
    lookups: i64,
    players: i64,
    client: Option<String>,
}

#[derive(FromRow)]
struct LookupBucket {
    bucket: i64,
    lookups: i64,
}

pub async fn flags(pool: &PgPool, redis: &RedisPool) -> Result<Vec<Flag>, AdminError> {
    Ok(blocked_accounts(pool, redis)
        .await?
        .into_iter()
        .map(|(block, discord_id)| waste_flag(block, discord_id))
        .collect())
}

pub async fn blocked_keys(
    pool: &PgPool,
    redis: &RedisPool,
    now: DateTime<Utc>,
) -> Result<BlockedKeys, AdminError> {
    let (blocked, hidden) = tokio::try_join!(blocked_accounts(pool, redis), dismissed(pool, now))?;
    let shown = blocked
        .into_iter()
        .filter(|(block, _)| !hidden.contains(&flag_key(block)));
    let items =
        try_join_all(shown.map(|(block, discord_id)| blocked_key(pool, block, discord_id))).await?;
    Ok(BlockedKeys {
        items,
        step_s: BUCKET.num_seconds(),
    })
}

async fn blocked_accounts(
    pool: &PgPool,
    redis: &RedisPool,
) -> Result<Vec<(Block, i64)>, AdminError> {
    let blocks = waste::blocks(&mut redis.connection())
        .await
        .context("read the keys blocked for wasting Hypixel calls")?;
    let members: Vec<i64> = blocks.iter().map(|block| block.member_id).collect();
    let discord_ids = discord_ids(pool, &members).await?;
    Ok(blocks
        .into_iter()
        .filter_map(|block| Some((block, *discord_ids.get(&block.member_id)?)))
        .collect())
}

async fn blocked_key(
    pool: &PgPool,
    block: Block,
    discord_id: i64,
) -> Result<BlockedKey, AdminError> {
    let (summary, buckets) = tokio::try_join!(
        lookup_summary(pool, block.member_id, block.blocked_at),
        lookup_buckets(pool, block.member_id, block.blocked_at),
    )?;
    Ok(BlockedKey {
        flag_key: flag_key(&block),
        account: EntityRef::Account {
            discord_id,
            member_id: Some(block.member_id),
        },
        blocked_at: block.blocked_at,
        until: block.until(),
        summary,
        series: fill_series(&buckets),
    })
}

async fn lookup_summary(
    pool: &PgPool,
    member_id: i64,
    blocked_at: DateTime<Utc>,
) -> Result<LookupSummary, AdminError> {
    sqlx::query_as(SUMMARY_SQL)
        .bind(member_id)
        .bind(blocked_at - LOOKBACK)
        .bind(blocked_at)
        .bind(KeyKind::Personal.code())
        .fetch_one(pool)
        .await
        .context("summarize the lookups before a waste block")
}

async fn lookup_buckets(
    pool: &PgPool,
    member_id: i64,
    blocked_at: DateTime<Utc>,
) -> Result<Vec<LookupBucket>, AdminError> {
    sqlx::query_as(BUCKETS_SQL)
        .bind(member_id)
        .bind(blocked_at - LOOKBACK)
        .bind(blocked_at)
        .bind(KeyKind::Personal.code())
        .bind(BUCKET.num_seconds() as f64)
        .fetch_all(pool)
        .await
        .context("chart the lookups before a waste block")
}

fn fill_series(buckets: &[LookupBucket]) -> Vec<i64> {
    let mut series = vec![0; bucket_count()];
    for bucket in buckets {
        if let Some(slot) = usize::try_from(bucket.bucket)
            .ok()
            .and_then(|index| series.get_mut(index))
        {
            *slot = bucket.lookups;
        }
    }
    series
}

fn bucket_count() -> usize {
    (LOOKBACK.num_seconds() / BUCKET.num_seconds()) as usize
}

fn flag_key(block: &Block) -> String {
    format!("waste:{}:{}", block.member_id, block.blocked_at.timestamp())
}

fn waste_flag(block: Block, discord_id: i64) -> Flag {
    Flag::new(
        FlagKind::Waste,
        FlagLevel::Danger,
        flag_key(&block),
        format!(
            "Personal key blocked until {}: its lookups made {} Hypixel calls on offline players that found nothing, faster than the {} a day it is allowed",
            block.until().format(UNTIL_FORMAT),
            grouped(ALLOWANCE as i64),
            grouped(REFILL_PER_DAY as i64),
        ),
    )
    .with_refs(vec![EntityRef::Account {
        discord_id,
        member_id: Some(block.member_id),
    }])
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn a_block_names_the_key_its_account_and_when_it_ends() {
        let block = Block {
            member_id: 7,
            blocked_at: Utc.with_ymd_and_hms(2026, 11, 2, 14, 5, 0).unwrap(),
        };
        let flag = waste_flag(block, 42);
        assert_eq!(flag.key, "waste:7:1793628300");
        assert_eq!(flag.level, FlagLevel::Danger);
        assert_eq!(
            flag.summary,
            "Personal key blocked until Nov 9, 14:05 UTC: its lookups made 500 Hypixel calls on offline players that found nothing, faster than the 2,500 a day it is allowed"
        );
        assert_eq!(
            flag.refs,
            vec![EntityRef::Account {
                discord_id: 42,
                member_id: Some(7)
            }]
        );
    }

    #[test]
    fn lookups_land_in_their_quarter_hour_of_the_day_before() {
        let series = fill_series(&[
            LookupBucket {
                bucket: 0,
                lookups: 3,
            },
            LookupBucket {
                bucket: 95,
                lookups: 9,
            },
            LookupBucket {
                bucket: 96,
                lookups: 1,
            },
        ]);
        assert_eq!(series.len(), 96);
        assert_eq!((series[0], series[95]), (3, 9));
        assert_eq!(series.iter().sum::<i64>(), 12);
    }
}
