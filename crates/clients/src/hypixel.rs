use std::time::Duration;

use chrono::{DateTime, Utc};
use coral_redis::minute_history::{HYPIXEL_ERRORS, HYPIXEL_HISTORY, HYPIXEL_REQUESTS};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use reqwest::header::HeaderMap;
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use crate::budget::{BudgetError, HypixelBudget, queue_hypixel_budget};
use crate::error::ClientError;
use crate::ratelimit::RateBudget;

const HYPIXEL_API_BASE: &str = "https://api.hypixel.net/v2";
const CACHE_TTL_SECS: u64 = 45;
const RESERVE_FILL: f64 = 0.95;
const MAX_ATTEMPTS: u32 = 3;
const DEFAULT_RETRY_SECS: i64 = 10;
const BUSY_BACKOFF: Duration = Duration::from_millis(500);
const LIMIT_HEADER: &str = "ratelimit-limit";
const REMAINING_HEADER: &str = "ratelimit-remaining";
const RESET_HEADER: &str = "ratelimit-reset";
const RETRY_AFTER_HEADER: &str = "retry-after";

#[derive(Clone)]
pub struct HypixelClient {
    http: Client,
    key: String,
    redis: ConnectionManager,
    budget: RateBudget,
}

#[derive(Debug, Deserialize)]
struct HypixelResponse {
    success: bool,
    #[serde(default)]
    cause: Option<String>,
    #[serde(default)]
    player: Option<Value>,
    #[serde(default)]
    guild: Option<Value>,
}

impl HypixelClient {
    pub fn new(key: String, redis: ConnectionManager) -> Result<Self, ClientError> {
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(ClientError::MissingApiKey);
        }
        let budget = RateBudget::new(redis.clone());
        Ok(Self {
            http: Client::new(),
            key,
            redis,
            budget,
        })
    }

    pub async fn get_player(&self, uuid: &str) -> Result<Option<Value>, ClientError> {
        let cache_key = format!("cache:hp:{}", uuid.to_lowercase());
        if let Some(cached) = self.cache_get(&cache_key).await {
            return Ok(cached);
        }

        let response = self
            .request(&format!("{HYPIXEL_API_BASE}/player?uuid={uuid}"))
            .await?;
        self.check_cause(&response)?;
        self.cache_set(&cache_key, &response.player).await;
        Ok(response.player)
    }

    pub async fn get_guild_by_player(&self, uuid: &str) -> Result<Option<Value>, ClientError> {
        self.guild(
            &format!("cache:hg:player:{}", uuid.to_lowercase()),
            &format!("{HYPIXEL_API_BASE}/guild?player={uuid}"),
        )
        .await
    }

    pub async fn get_guild_by_id(&self, id: &str) -> Result<Option<Value>, ClientError> {
        self.guild(
            &format!("cache:hg:id:{id}"),
            &format!("{HYPIXEL_API_BASE}/guild?id={id}"),
        )
        .await
    }

    pub async fn get_guild_by_name(&self, name: &str) -> Result<Option<Value>, ClientError> {
        self.guild(
            &format!("cache:hg:name:{}", name.to_lowercase()),
            &format!("{HYPIXEL_API_BASE}/guild?name={name}"),
        )
        .await
    }

    async fn guild(&self, cache_key: &str, url: &str) -> Result<Option<Value>, ClientError> {
        if let Some(cached) = self.cache_get(cache_key).await {
            return Ok(cached);
        }
        let response = self.request(url).await?;
        self.check_cause(&response)?;
        self.cache_set(cache_key, &response.guild).await;
        Ok(response.guild)
    }

    async fn request(&self, url: &str) -> Result<HypixelResponse, ClientError> {
        for attempt in 1..=MAX_ATTEMPTS {
            if !self.budget.try_reserve(&self.key, RESERVE_FILL).await {
                if attempt == MAX_ATTEMPTS {
                    return Err(ClientError::RateLimited);
                }
                tokio::time::sleep(BUSY_BACKOFF).await;
                continue;
            }

            let response = self
                .http
                .get(url)
                .header("API-Key", &self.key)
                .send()
                .await?;
            self.record_upstream(&response).await;

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry = header_i64(response.headers(), RETRY_AFTER_HEADER)
                    .unwrap_or(DEFAULT_RETRY_SECS);
                self.budget.penalize(&self.key, retry).await;
                if attempt == MAX_ATTEMPTS {
                    return Err(ClientError::RateLimited);
                }
                tokio::time::sleep(Duration::from_secs(retry.max(1) as u64)).await;
                continue;
            }

            self.budget
                .record(
                    &self.key,
                    header_i64(response.headers(), LIMIT_HEADER),
                    header_i64(response.headers(), REMAINING_HEADER),
                    header_i64(response.headers(), RESET_HEADER),
                )
                .await;
            return Ok(response.json().await?);
        }
        Err(ClientError::RateLimited)
    }

    fn check_cause(&self, response: &HypixelResponse) -> Result<(), ClientError> {
        if response.success {
            return Ok(());
        }
        match &response.cause {
            Some(cause) if cause.contains("limit") => Err(ClientError::RateLimited),
            Some(cause) => Err(ClientError::HypixelApi(cause.clone())),
            None => Ok(()),
        }
    }

    async fn record_upstream(&self, response: &Response) {
        let mut redis = self.redis.clone();
        let recorded = record_upstream_at(
            &mut redis,
            response.status(),
            response.headers(),
            Utc::now(),
        )
        .await;
        if let Err(e) = recorded {
            tracing::warn!("failed to record a Hypixel request: {e}");
        }
    }

    async fn cache_get(&self, key: &str) -> Option<Option<Value>> {
        let json: String = self.redis.clone().get(key).await.ok()?;
        serde_json::from_str(&json).ok()
    }

    async fn cache_set(&self, key: &str, value: &Option<Value>) {
        if let Ok(json) = serde_json::to_string(value) {
            let _: Result<(), _> = self.redis.clone().set_ex(key, json, CACHE_TTL_SECS).await;
        }
    }
}

async fn record_upstream_at(
    redis: &mut ConnectionManager,
    status: StatusCode,
    headers: &HeaderMap,
    at: DateTime<Utc>,
) -> Result<(), BudgetError> {
    let mut pipe = redis::pipe();
    HYPIXEL_HISTORY.queue(&mut pipe, at, upstream_counters(status));
    if let Some(budget) = observed_budget(headers, at) {
        queue_hypixel_budget(&mut pipe, &budget)?;
    }
    pipe.query_async::<()>(redis).await?;
    Ok(())
}

fn upstream_counters(status: StatusCode) -> &'static [&'static str] {
    if status.is_success() {
        &[HYPIXEL_REQUESTS]
    } else {
        &[HYPIXEL_REQUESTS, HYPIXEL_ERRORS]
    }
}

fn observed_budget(headers: &HeaderMap, at: DateTime<Utc>) -> Option<HypixelBudget> {
    let limit = header_i64(headers, LIMIT_HEADER).filter(|&limit| limit > 0)?;
    let remaining = header_i64(headers, REMAINING_HEADER)?;
    Some(HypixelBudget {
        used: limit.saturating_sub(remaining).clamp(0, limit),
        limit,
        at,
    })
}

fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use redis::Client;
    use reqwest::header::HeaderValue;

    use super::*;
    use crate::budget::{HYPIXEL_BUDGET_KEY, hypixel_budget};

    const SAMPLE_INSTANT: &str = "1970-01-03T00:07:00Z";

    async fn test_redis() -> Option<ConnectionManager> {
        let url = std::env::var("REDIS_URL").ok()?;
        ConnectionManager::new(Client::open(url).ok()?).await.ok()
    }

    fn instant() -> DateTime<Utc> {
        SAMPLE_INSTANT.parse().unwrap()
    }

    fn rate_headers(limit: &str, remaining: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(LIMIT_HEADER, HeaderValue::from_str(limit).unwrap());
        if let Some(remaining) = remaining {
            headers.insert(REMAINING_HEADER, HeaderValue::from_str(remaining).unwrap());
        }
        headers
    }

    fn observed(limit: &str, remaining: Option<&str>) -> Option<(i64, i64)> {
        observed_budget(&rate_headers(limit, remaining), instant())
            .map(|budget| (budget.used, budget.limit))
    }

    #[test]
    fn budget_counts_what_the_window_has_spent() {
        assert_eq!(observed("300", Some("64")), Some((236, 300)));
    }

    #[test]
    fn budget_needs_both_rate_headers() {
        assert_eq!(observed("300", None), None);
    }

    #[test]
    fn budget_ignores_a_non_positive_limit() {
        assert_eq!(observed("0", Some("0")), None);
        assert_eq!(observed("-5", Some("0")), None);
    }

    #[test]
    fn budget_stays_within_the_limit() {
        assert_eq!(observed("300", Some("400")), Some((0, 300)));
        assert_eq!(observed("300", Some("-20")), Some((300, 300)));
    }

    #[tokio::test]
    async fn an_upstream_call_counts_its_minute_and_publishes_the_budget() {
        let Some(mut redis) = test_redis().await else {
            return;
        };
        let at = instant();
        let slot = HYPIXEL_HISTORY.slot(at);
        let written = [slot.key.as_str(), HYPIXEL_BUDGET_KEY];
        let () = redis.del(&written).await.unwrap();

        let spent = rate_headers("300", Some("64"));
        record_upstream_at(&mut redis, StatusCode::OK, &spent, at)
            .await
            .unwrap();
        record_upstream_at(&mut redis, StatusCode::BAD_GATEWAY, &HeaderMap::new(), at)
            .await
            .unwrap();

        let counts: HashMap<String, i64> = redis.hgetall(&slot.key).await.unwrap();
        let budget = hypixel_budget(&mut redis).await.unwrap();
        let () = redis.del(&written).await.unwrap();
        assert_eq!(
            counts,
            HashMap::from([("t7".to_string(), 2), ("e7".to_string(), 1)])
        );
        assert_eq!(
            budget,
            Some(HypixelBudget {
                used: 236,
                limit: 300,
                at,
            })
        );
    }
}
