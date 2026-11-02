use std::fmt;
use std::sync::LazyLock;

use chrono::{DateTime, TimeDelta, Utc};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;

pub const ALLOWANCE: f64 = 500.0;
pub const REFILL_PER_DAY: f64 = 2_500.0;
pub const BLOCK_FOR: TimeDelta = TimeDelta::days(7);

const SECS_PER_DAY: f64 = 86_400.0;
const ALLOWANCE_PREFIX: &str = "waste:allowance:";
const BLOCK_PREFIX: &str = "waste:block:";
const PERSONAL: &str = "personal";
const DEVELOPER: &str = "developer";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyHolder {
    Personal(i64),
    Developer(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    Within,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub holder: KeyHolder,
    pub blocked_at: DateTime<Utc>,
}

impl KeyHolder {
    pub fn member_id(self) -> i64 {
        match self {
            Self::Personal(member) | Self::Developer(member) => member,
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        let (kind, member) = raw.split_once(':')?;
        let member = member.parse().ok()?;
        match kind {
            PERSONAL => Some(Self::Personal(member)),
            DEVELOPER => Some(Self::Developer(member)),
            _ => None,
        }
    }
}

impl fmt::Display for KeyHolder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Personal(member) => write!(f, "{PERSONAL}:{member}"),
            Self::Developer(member) => write!(f, "{DEVELOPER}:{member}"),
        }
    }
}

impl Block {
    pub fn until(&self) -> DateTime<Utc> {
        self.blocked_at + BLOCK_FOR
    }
}

pub async fn charge(
    conn: &mut ConnectionManager,
    holder: KeyHolder,
    now: DateTime<Utc>,
) -> redis::RedisResult<Charge> {
    let blocked: i64 = CHARGE
        .key(allowance_key(holder))
        .key(block_key(holder))
        .arg(now.timestamp())
        .arg(ALLOWANCE)
        .arg(REFILL_PER_DAY / SECS_PER_DAY)
        .arg(BLOCK_FOR.num_seconds())
        .invoke_async(conn)
        .await?;
    Ok(match blocked {
        1 => Charge::Blocked,
        _ => Charge::Within,
    })
}

pub async fn is_blocked(
    conn: &mut ConnectionManager,
    holder: KeyHolder,
) -> redis::RedisResult<bool> {
    conn.exists(block_key(holder)).await
}

pub async fn blocks(conn: &mut ConnectionManager) -> redis::RedisResult<Vec<Block>> {
    let mut keys: Vec<String> = Vec::new();
    let mut found = conn
        .scan_match::<_, String>(format!("{BLOCK_PREFIX}*"))
        .await?;
    while let Some(key) = found.next_item().await {
        keys.push(key);
    }
    drop(found);
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let blocked_at: Vec<Option<i64>> = conn.mget(&keys).await?;
    Ok(keys
        .iter()
        .zip(blocked_at)
        .filter_map(|(key, at)| {
            Some(Block {
                holder: KeyHolder::parse(key.strip_prefix(BLOCK_PREFIX)?)?,
                blocked_at: DateTime::from_timestamp(at?, 0)?,
            })
        })
        .collect())
}

pub async fn lift(conn: &mut ConnectionManager, holder: KeyHolder) -> redis::RedisResult<bool> {
    let (lifted, _): (i64, i64) = redis::pipe()
        .del(block_key(holder))
        .del(allowance_key(holder))
        .query_async(conn)
        .await?;
    Ok(lifted == 1)
}

fn allowance_key(holder: KeyHolder) -> String {
    format!("{ALLOWANCE_PREFIX}{holder}")
}

fn block_key(holder: KeyHolder) -> String {
    format!("{BLOCK_PREFIX}{holder}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RedisPool;

    async fn test_conn() -> Option<ConnectionManager> {
        let url = std::env::var("REDIS_URL").ok()?;
        Some(RedisPool::connect(&url).await.ok()?.connection())
    }

    fn fresh_holder() -> KeyHolder {
        KeyHolder::Developer(i64::from(rand::random::<u32>()) + i64::from(u32::MAX))
    }

    #[test]
    fn holders_round_trip_through_their_key() {
        for holder in [KeyHolder::Personal(12), KeyHolder::Developer(7)] {
            assert_eq!(KeyHolder::parse(&holder.to_string()), Some(holder));
        }
        assert_eq!(KeyHolder::parse("session:3"), None);
    }

    #[tokio::test]
    async fn spending_the_whole_allowance_at_once_blocks_the_key() {
        let Some(mut conn) = test_conn().await else {
            return;
        };
        let holder = fresh_holder();
        let now = Utc::now();
        for _ in 1..ALLOWANCE as i64 {
            assert_eq!(
                charge(&mut conn, holder, now).await.unwrap(),
                Charge::Within
            );
        }
        assert!(!is_blocked(&mut conn, holder).await.unwrap());
        assert_eq!(
            charge(&mut conn, holder, now).await.unwrap(),
            Charge::Blocked
        );
        assert!(is_blocked(&mut conn, holder).await.unwrap());
        assert!(
            blocks(&mut conn)
                .await
                .unwrap()
                .iter()
                .any(|b| b.holder == holder)
        );
        assert!(lift(&mut conn, holder).await.unwrap());
        assert!(!is_blocked(&mut conn, holder).await.unwrap());
    }

    #[tokio::test]
    async fn waste_spread_below_the_refill_rate_never_blocks() {
        let Some(mut conn) = test_conn().await else {
            return;
        };
        let holder = fresh_holder();
        let gap = TimeDelta::seconds((SECS_PER_DAY / REFILL_PER_DAY).ceil() as i64);
        let start = Utc::now();
        for call in 0..2 * ALLOWANCE as i32 {
            let at = start + gap * call;
            assert_eq!(charge(&mut conn, holder, at).await.unwrap(), Charge::Within);
        }
        lift(&mut conn, holder).await.unwrap();
    }
}
