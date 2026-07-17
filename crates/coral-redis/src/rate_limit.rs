use redis::AsyncCommands;

use crate::RedisPool;

const WINDOW_SECS: i64 = 300;
const KEY_PREFIX: &str = "ratelimit:";

pub const PERSONAL_RATE_LIMIT: i64 = 600;
pub const SESSION_RATE_LIMIT: i64 = 300;
pub const SESSION_UUID_BUDGET: i64 = 2000;

pub enum RateLimitResult {
    Allowed { remaining: i64 },
    Exceeded,
}

#[derive(Debug, Clone)]
pub struct BudgetUsage {
    pub name: String,
    pub used: i64,
    pub limit: i64,
}

impl BudgetUsage {
    pub fn utilization(&self) -> f64 {
        if self.limit <= 0 {
            0.0
        } else {
            self.used as f64 / self.limit as f64
        }
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    pool: RedisPool,
}

impl RateLimiter {
    pub fn new(pool: RedisPool) -> Self {
        Self { pool }
    }

    pub async fn check_and_record(
        &self,
        api_key: &str,
        limit: i64,
    ) -> Result<RateLimitResult, redis::RedisError> {
        self.check_and_record_weighted(api_key, limit, 1).await
    }

    pub async fn check_and_record_weighted(
        &self,
        api_key: &str,
        limit: i64,
        weight: i64,
    ) -> Result<RateLimitResult, redis::RedisError> {
        let key = format!("{KEY_PREFIX}{api_key}");
        let now = chrono::Utc::now().timestamp();
        let mut conn = self.pool.connection();

        let mut pipe = redis::pipe();
        pipe.atomic()
            .cmd("ZREMRANGEBYSCORE")
            .arg(&key)
            .arg("-inf")
            .arg(now - WINDOW_SECS)
            .ignore();
        for _ in 0..weight.max(1) {
            pipe.cmd("ZADD")
                .arg(&key)
                .arg(now)
                .arg(format!("{now}:{}", uuid::Uuid::new_v4()))
                .ignore();
        }
        pipe.cmd("EXPIRE")
            .arg(&key)
            .arg(WINDOW_SECS + 10)
            .ignore()
            .query_async::<()>(&mut conn)
            .await?;

        let count: i64 = conn.zcard(&key).await?;
        match count > limit {
            true => Ok(RateLimitResult::Exceeded),
            false => Ok(RateLimitResult::Allowed {
                remaining: limit - count,
            }),
        }
    }

    pub async fn check_tag_limit(
        &self,
        discord_id: i64,
        access_level: i16,
    ) -> Result<RateLimitResult, redis::RedisError> {
        let limit = tag_limit_for_access(access_level);
        if limit == 0 {
            return Ok(RateLimitResult::Allowed {
                remaining: i64::MAX,
            });
        }
        self.check_and_record(&format!("tag:{discord_id}"), limit)
            .await
    }

    pub async fn reset(&self, name: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.pool.connection();
        conn.del::<_, ()>(key(name)).await
    }

    pub async fn usage_many(&self, names: &[String]) -> Result<Vec<i64>, redis::RedisError> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.pool.connection();
        let mut pipe = redis::pipe();
        for name in names {
            pipe.cmd("ZCARD").arg(key(name));
        }
        pipe.query_async(&mut conn).await
    }

    pub async fn scan_budgets(
        &self,
        name_prefix: &str,
        limit: i64,
    ) -> Result<Vec<BudgetUsage>, redis::RedisError> {
        let mut conn = self.pool.connection();
        let pattern = key(&format!("{name_prefix}*"));
        let mut cursor: u64 = 0;
        let mut keys = Vec::new();
        loop {
            let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(200)
                .query_async(&mut conn)
                .await?;
            keys.extend(batch);
            cursor = next;
            if cursor == 0 {
                break;
            }
        }

        let mut usages = Vec::with_capacity(keys.len());
        for redis_key in keys {
            let used: i64 = conn.zcard(&redis_key).await?;
            let name = redis_key
                .strip_prefix(KEY_PREFIX)
                .unwrap_or(&redis_key)
                .to_string();
            usages.push(BudgetUsage { name, used, limit });
        }
        Ok(usages)
    }
}

pub fn key(name: &str) -> String {
    format!("{KEY_PREFIX}{name}")
}

fn tag_limit_for_access(access_level: i16) -> i64 {
    match access_level {
        5.. => 0,
        3..=4 => 60,
        2 => 30,
        1 => 15,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_applies_shared_prefix() {
        assert_eq!(key("sf:12345"), "ratelimit:sf:12345");
        assert_eq!(key("sfuuids:12345"), "ratelimit:sfuuids:12345");
    }

    #[test]
    fn tag_limit_tiers_by_access_level() {
        assert_eq!(tag_limit_for_access(0), 10);
        assert_eq!(tag_limit_for_access(1), 15);
        assert_eq!(tag_limit_for_access(2), 30);
        assert_eq!(tag_limit_for_access(3), 60);
        assert_eq!(tag_limit_for_access(4), 60);
        assert_eq!(tag_limit_for_access(5), 0);
        assert_eq!(tag_limit_for_access(10), 0);
    }
}
