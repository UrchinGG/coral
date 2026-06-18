use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

use redis::aio::ConnectionManager;

const DEFAULT_LIMIT: i64 = 120;
const DEFAULT_WINDOW: i64 = 300;

static RESERVE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r"
        local cur = tonumber(redis.call('GET', KEYS[1]) or '0')
        local lim = tonumber(redis.call('GET', KEYS[2]) or ARGV[1])
        local cap = math.floor(lim * tonumber(ARGV[2]) / 1000000)
        if cur < cap then
            local n = redis.call('INCR', KEYS[1])
            if n == 1 then redis.call('EXPIRE', KEYS[1], ARGV[3]) end
            return 1
        end
        return 0
        ",
    )
});

static RECORD: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r"
        local reset = math.max(tonumber(ARGV[3]), 1)
        redis.call('SET', KEYS[2], ARGV[1], 'EX', reset)
        local used = tonumber(ARGV[1]) - tonumber(ARGV[2])
        if used < 0 then used = 0 end
        local cur = tonumber(redis.call('GET', KEYS[1]) or '0')
        if used > cur then
            redis.call('SET', KEYS[1], used, 'EX', reset)
        else
            redis.call('EXPIRE', KEYS[1], reset)
        end
        return 1
        ",
    )
});

#[derive(Clone)]
pub struct RateBudget {
    redis: ConnectionManager,
}

impl RateBudget {
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    pub async fn try_reserve(&self, api_key: &str, fill: f64) -> bool {
        let (counter, limit) = bucket_keys(api_key);
        let fill_micro = (fill.clamp(0.0, 1.0) * 1_000_000.0) as i64;
        RESERVE
            .key(counter)
            .key(limit)
            .arg(DEFAULT_LIMIT)
            .arg(fill_micro)
            .arg(DEFAULT_WINDOW)
            .invoke_async::<i64>(&mut self.redis.clone())
            .await
            .unwrap_or(1)
            == 1
    }

    pub async fn record(
        &self,
        api_key: &str,
        limit: Option<i64>,
        remaining: Option<i64>,
        reset_secs: Option<i64>,
    ) {
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        let remaining = remaining.unwrap_or(limit);
        let reset = reset_secs.unwrap_or(DEFAULT_WINDOW);
        let (counter, limit_key) = bucket_keys(api_key);
        let _: Result<i64, _> = RECORD
            .key(counter)
            .key(limit_key)
            .arg(limit)
            .arg(remaining)
            .arg(reset)
            .invoke_async(&mut self.redis.clone())
            .await;
    }

    pub async fn penalize(&self, api_key: &str, retry_secs: i64) {
        let (counter, _) = bucket_keys(api_key);
        let _: Result<(), _> = redis::cmd("SET")
            .arg(&counter)
            .arg(i64::MAX)
            .arg("EX")
            .arg(retry_secs.max(1))
            .query_async(&mut self.redis.clone())
            .await;
    }
}

fn bucket_keys(api_key: &str) -> (String, String) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    api_key.hash(&mut hasher);
    let id = format!("{:016x}", hasher.finish());
    (format!("hp:rl:{id}:n"), format!("hp:rl:{id}:lim"))
}
