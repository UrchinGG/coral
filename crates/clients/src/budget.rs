use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Pipeline};
use serde::{Deserialize, Serialize};

use crate::serde_ts;

pub const HYPIXEL_BUDGET_KEY: &str = "hp:budget";
pub const BUDGET_TTL_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypixelBudget {
    pub used: i64,
    pub limit: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    #[error("Redis command failed: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("budget JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
}

pub async fn hypixel_budget(
    redis: &mut ConnectionManager,
) -> Result<Option<HypixelBudget>, BudgetError> {
    read_budget(redis, HYPIXEL_BUDGET_KEY).await
}

pub async fn publish_hypixel_budget(
    redis: &mut ConnectionManager,
    budget: &HypixelBudget,
) -> Result<(), BudgetError> {
    write_budget(redis, HYPIXEL_BUDGET_KEY, budget).await
}

pub(crate) fn queue_hypixel_budget(
    pipe: &mut Pipeline,
    budget: &HypixelBudget,
) -> Result<(), BudgetError> {
    queue_budget(pipe, HYPIXEL_BUDGET_KEY, budget)
}

async fn read_budget(
    redis: &mut ConnectionManager,
    key: &str,
) -> Result<Option<HypixelBudget>, BudgetError> {
    let stored: Option<String> = redis.get(key).await?;
    let budget = stored.map(|json| serde_json::from_str(&json)).transpose()?;
    Ok(budget)
}

async fn write_budget(
    redis: &mut ConnectionManager,
    key: &str,
    budget: &HypixelBudget,
) -> Result<(), BudgetError> {
    let mut pipe = redis::pipe();
    queue_budget(&mut pipe, key, budget)?;
    pipe.query_async::<()>(redis).await?;
    Ok(())
}

fn queue_budget(pipe: &mut Pipeline, key: &str, budget: &HypixelBudget) -> Result<(), BudgetError> {
    let json = serde_json::to_string(budget)?;
    pipe.set_ex(key, json, BUDGET_TTL_SECS).ignore();
    Ok(())
}

#[cfg(test)]
mod tests {
    use redis::Client;
    use serde_json::json;

    use super::*;

    const ROUND_TRIP_KEY: &str = "test:hp:budget:round-trip";
    const ABSENT_KEY: &str = "test:hp:budget:absent";
    const MALFORMED_KEY: &str = "test:hp:budget:malformed";
    const SAMPLE_INSTANT: &str = "2026-10-14T12:30:00.25Z";

    async fn test_redis() -> Option<ConnectionManager> {
        let url = std::env::var("REDIS_URL").ok()?;
        ConnectionManager::new(Client::open(url).ok()?).await.ok()
    }

    fn sample() -> HypixelBudget {
        HypixelBudget {
            used: 236,
            limit: 300,
            at: SAMPLE_INSTANT.parse().unwrap(),
        }
    }

    #[test]
    fn budget_serializes_used_limit_and_microsecond_instant() {
        assert_eq!(
            serde_json::to_value(sample()).unwrap(),
            json!({ "used": 236, "limit": 300, "at": "2026-10-14T12:30:00.250000Z" })
        );
    }

    #[tokio::test]
    async fn published_budget_reads_back_and_expires() {
        let Some(mut redis) = test_redis().await else {
            return;
        };
        write_budget(&mut redis, ROUND_TRIP_KEY, &sample())
            .await
            .unwrap();

        assert_eq!(
            read_budget(&mut redis, ROUND_TRIP_KEY).await.unwrap(),
            Some(sample())
        );
        let ttl: i64 = redis.ttl(ROUND_TRIP_KEY).await.unwrap();
        assert!(ttl > 0 && ttl <= BUDGET_TTL_SECS as i64, "{ttl}");
        let () = redis.del(ROUND_TRIP_KEY).await.unwrap();
    }

    #[tokio::test]
    async fn absent_budget_reads_as_none() {
        let Some(mut redis) = test_redis().await else {
            return;
        };
        let () = redis.del(ABSENT_KEY).await.unwrap();
        assert_eq!(read_budget(&mut redis, ABSENT_KEY).await.unwrap(), None);
    }

    #[tokio::test]
    async fn malformed_budget_is_an_error() {
        let Some(mut redis) = test_redis().await else {
            return;
        };
        let () = redis.set(MALFORMED_KEY, "not json").await.unwrap();
        let error = read_budget(&mut redis, MALFORMED_KEY).await.unwrap_err();
        assert!(matches!(error, BudgetError::Json(_)), "{error}");
        let () = redis.del(MALFORMED_KEY).await.unwrap();
    }
}
