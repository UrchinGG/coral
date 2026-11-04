use std::sync::LazyLock;

use chrono::{DateTime, TimeDelta, Utc};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;

pub const ALLOWANCE: f64 = 500.0;
pub const REFILL_PER_DAY: f64 = 2_500.0;
pub const BLOCK_FOR: TimeDelta = TimeDelta::days(7);

const SECS_PER_DAY: f64 = 86_400.0;
const ALLOWANCE_PREFIX: &str = "waste:allowance:personal:";
const BLOCK_PREFIX: &str = "waste:block:personal:";

static CHARGE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r"
        local now = tonumber(ARGV[1])
        local allowance = tonumber(ARGV[2])
        local refill = tonumber(ARGV[3])
        local spent = tonumber(redis.call('HGET', KEYS[1], 'spent')) or 0
        local at = tonumber(redis.call('HGET', KEYS[1], 'at')) or now
        spent = math.max(0, spent - (now - at) * refill) + 1
        if spent < allowance then
            redis.call('HSET', KEYS[1], 'spent', spent, 'at', now)
            redis.call('EXPIRE', KEYS[1], math.ceil(spent / refill))
            return 0
        end
        redis.call('DEL', KEYS[1])
        if redis.call('SET', KEYS[2], now, 'EX', ARGV[4], 'NX') then
            return 1
        end
        return 0
        ",
    )
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    Within,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub member_id: i64,
    pub blocked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Allowance {
    pub member_id: i64,
    pub spent: f64,
}

impl Block {
    pub fn until(&self) -> DateTime<Utc> {
        self.blocked_at + BLOCK_FOR
    }
}

pub async fn charge(
    conn: &mut ConnectionManager,
    member_id: i64,
    now: DateTime<Utc>,
) -> redis::RedisResult<Charge> {
    let blocked: i64 = CHARGE
        .key(allowance_key(member_id))
        .key(block_key(member_id))
        .arg(now.timestamp())
        .arg(ALLOWANCE)
        .arg(refill_per_second())
        .arg(BLOCK_FOR.num_seconds())
        .invoke_async(conn)
        .await?;
    Ok(match blocked {
        1 => Charge::Blocked,
        _ => Charge::Within,
    })
}

pub async fn is_blocked(conn: &mut ConnectionManager, member_id: i64) -> redis::RedisResult<bool> {
    conn.exists(block_key(member_id)).await
}

pub async fn block(
    conn: &mut ConnectionManager,
    member_id: i64,
) -> redis::RedisResult<Option<Block>> {
    let blocked_at: Option<i64> = conn.get(block_key(member_id)).await?;
    Ok(blocked_at
        .and_then(|at| DateTime::from_timestamp(at, 0))
        .map(|blocked_at| Block {
            member_id,
            blocked_at,
        }))
}

pub async fn blocks(conn: &mut ConnectionManager) -> redis::RedisResult<Vec<Block>> {
    let keys = keys_with_prefix(conn, BLOCK_PREFIX).await?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let blocked_at: Vec<Option<i64>> = conn.mget(&keys).await?;
    Ok(keys
        .iter()
        .zip(blocked_at)
        .filter_map(|(key, at)| {
            Some(Block {
                member_id: key.strip_prefix(BLOCK_PREFIX)?.parse().ok()?,
                blocked_at: DateTime::from_timestamp(at?, 0)?,
            })
        })
        .collect())
}

pub async fn spent(
    conn: &mut ConnectionManager,
    member_id: i64,
    now: DateTime<Utc>,
) -> redis::RedisResult<f64> {
    let (spent, at): (Option<f64>, Option<i64>) = redis::cmd("HMGET")
        .arg(allowance_key(member_id))
        .arg("spent")
        .arg("at")
        .query_async(conn)
        .await?;
    Ok(drained(spent, at, now))
}

pub async fn allowances(
    conn: &mut ConnectionManager,
    now: DateTime<Utc>,
) -> redis::RedisResult<Vec<Allowance>> {
    let keys = keys_with_prefix(conn, ALLOWANCE_PREFIX).await?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let mut pipe = redis::pipe();
    for key in &keys {
        pipe.cmd("HMGET").arg(key).arg("spent").arg("at");
    }
    let readings: Vec<(Option<f64>, Option<i64>)> = pipe.query_async(conn).await?;
    Ok(keys
        .iter()
        .zip(readings)
        .filter_map(|(key, (spent, at))| {
            Some(Allowance {
                member_id: key.strip_prefix(ALLOWANCE_PREFIX)?.parse().ok()?,
                spent: drained(spent, at, now),
            })
        })
        .filter(|allowance| allowance.spent > 0.0)
        .collect())
}

pub async fn lift(conn: &mut ConnectionManager, member_id: i64) -> redis::RedisResult<bool> {
    let (lifted, _): (i64, i64) = redis::pipe()
        .del(block_key(member_id))
        .del(allowance_key(member_id))
        .query_async(conn)
        .await?;
    Ok(lifted == 1)
}

async fn keys_with_prefix(
    conn: &mut ConnectionManager,
    prefix: &str,
) -> redis::RedisResult<Vec<String>> {
    let mut keys = Vec::new();
    let mut found = conn.scan_match::<_, String>(format!("{prefix}*")).await?;
    while let Some(key) = found.next_item().await {
        keys.push(key);
    }
    Ok(keys)
}

fn drained(spent: Option<f64>, at: Option<i64>, now: DateTime<Utc>) -> f64 {
    match (spent, at) {
        (Some(spent), Some(at)) => {
            (spent - (now.timestamp() - at) as f64 * refill_per_second()).max(0.0)
        }
        _ => 0.0,
    }
}

fn refill_per_second() -> f64 {
    REFILL_PER_DAY / SECS_PER_DAY
}

fn allowance_key(member_id: i64) -> String {
    format!("{ALLOWANCE_PREFIX}{member_id}")
}

fn block_key(member_id: i64) -> String {
    format!("{BLOCK_PREFIX}{member_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RedisPool;

    async fn test_conn() -> Option<ConnectionManager> {
        let url = std::env::var("REDIS_URL").ok()?;
        Some(RedisPool::connect(&url).await.ok()?.connection())
    }

    #[test]
    fn spent_allowance_drains_at_the_refill_rate() {
        let start = DateTime::from_timestamp(0, 0).unwrap();
        let fifty_refilled = start + TimeDelta::seconds((50.0 / refill_per_second()) as i64);
        assert_eq!(drained(Some(100.0), Some(0), start), 100.0);
        assert!((drained(Some(100.0), Some(0), fifty_refilled) - 50.0).abs() < 0.01);
        assert_eq!(
            drained(Some(10.0), Some(0), start + TimeDelta::days(1)),
            0.0
        );
        assert_eq!(drained(None, None, start), 0.0);
    }

    fn fresh_member() -> i64 {
        i64::from(rand::random::<u32>()) + i64::from(u32::MAX)
    }

    #[tokio::test]
    async fn spending_the_whole_allowance_at_once_blocks_the_key() {
        let Some(mut conn) = test_conn().await else {
            return;
        };
        let member = fresh_member();
        let now = Utc::now();
        for _ in 1..ALLOWANCE as i64 {
            assert_eq!(
                charge(&mut conn, member, now).await.unwrap(),
                Charge::Within
            );
        }
        assert!(!is_blocked(&mut conn, member).await.unwrap());
        assert_eq!(
            charge(&mut conn, member, now).await.unwrap(),
            Charge::Blocked
        );
        assert!(is_blocked(&mut conn, member).await.unwrap());
        assert!(
            blocks(&mut conn)
                .await
                .unwrap()
                .iter()
                .any(|b| b.member_id == member)
        );
        assert_eq!(
            block(&mut conn, member).await.unwrap().map(|b| b.member_id),
            Some(member)
        );
        assert!(lift(&mut conn, member).await.unwrap());
        assert_eq!(block(&mut conn, member).await.unwrap(), None);
        assert!(!is_blocked(&mut conn, member).await.unwrap());
    }

    #[tokio::test]
    async fn waste_spread_below_the_refill_rate_never_blocks() {
        let Some(mut conn) = test_conn().await else {
            return;
        };
        let member = fresh_member();
        let gap = TimeDelta::seconds((SECS_PER_DAY / REFILL_PER_DAY).ceil() as i64);
        let start = Utc::now();
        for call in 0..2 * ALLOWANCE as i32 {
            let at = start + gap * call;
            assert_eq!(charge(&mut conn, member, at).await.unwrap(), Charge::Within);
        }
        lift(&mut conn, member).await.unwrap();
    }
}
