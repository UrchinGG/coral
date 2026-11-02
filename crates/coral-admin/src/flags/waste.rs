use std::collections::HashMap;

use clients::FlagLevel;
use coral_redis::RedisPool;
use coral_redis::waste::{self, ALLOWANCE, Block, REFILL_PER_DAY};
use sqlx::PgPool;

use super::{Flag, FlagKind, grouped};
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

const UNTIL_FORMAT: &str = "%b %-d, %H:%M UTC";

pub async fn flags(pool: &PgPool, redis: &RedisPool) -> Result<Vec<Flag>, AdminError> {
    let blocks = waste::blocks(&mut redis.connection())
        .await
        .context("read the keys blocked for wasting Hypixel calls")?;
    let members: Vec<i64> = blocks.iter().map(|block| block.member_id).collect();
    let discord_ids = discord_ids(pool, &members).await?;
    Ok(blocks
        .into_iter()
        .filter_map(|block| {
            let discord_id = *discord_ids.get(&block.member_id)?;
            Some(waste_flag(block, discord_id))
        })
        .collect())
}

async fn discord_ids(pool: &PgPool, members: &[i64]) -> Result<HashMap<i64, i64>, AdminError> {
    let rows: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, discord_id FROM members WHERE id = ANY($1)")
            .bind(members)
            .fetch_all(pool)
            .await
            .context("find the accounts of blocked keys")?;
    Ok(rows.into_iter().collect())
}

fn waste_flag(block: Block, discord_id: i64) -> Flag {
    Flag::new(
        FlagKind::Waste,
        FlagLevel::Danger,
        format!("waste:{}:{}", block.member_id, block.blocked_at.timestamp()),
        format!(
            "Personal key blocked until {}: its lookups made {} Hypixel calls that found nothing, faster than the {} a day it is allowed",
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
            "Personal key blocked until Nov 9, 14:05 UTC: its lookups made 500 Hypixel calls that found nothing, faster than the 2,500 a day it is allowed"
        );
        assert_eq!(
            flag.refs,
            vec![EntityRef::Account {
                discord_id: 42,
                member_id: Some(7)
            }]
        );
    }
}
