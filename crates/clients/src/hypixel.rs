use std::time::Duration;

use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use crate::error::ClientError;
use crate::ratelimit::RateBudget;

const HYPIXEL_API_BASE: &str = "https://api.hypixel.net/v2";
const CACHE_TTL_SECS: u64 = 45;
const RESERVE_FILL: f64 = 0.95;
const MAX_ATTEMPTS: u32 = 3;
const DEFAULT_RETRY_SECS: i64 = 10;
const BUSY_BACKOFF: Duration = Duration::from_millis(500);

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
            return Err(ClientError::NoApiKeys);
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

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry = header_i64(&response, "retry-after").unwrap_or(DEFAULT_RETRY_SECS);
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
                    header_i64(&response, "ratelimit-limit"),
                    header_i64(&response, "ratelimit-remaining"),
                    header_i64(&response, "ratelimit-reset"),
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

fn header_i64(response: &Response, name: &str) -> Option<i64> {
    response.headers().get(name)?.to_str().ok()?.parse().ok()
}
