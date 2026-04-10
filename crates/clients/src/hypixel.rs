use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::error::ClientError;

const HYPIXEL_API_BASE: &str = "https://api.hypixel.net/v2";
const CACHE_TTL_SECS: u64 = 45;


#[derive(Clone)]
pub struct HypixelClient {
    http: Client,
    key: String,
    redis: ConnectionManager,
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
        if key.is_empty() {
            return Err(ClientError::NoApiKeys);
        }

        Ok(Self { http: Client::new(), key, redis })
    }

    pub async fn get_player(&self, uuid: &str) -> Result<Option<Value>, ClientError> {
        let cache_key = format!("cache:hp:{}", uuid.to_lowercase());

        if let Some(cached) = self.cache_get(&cache_key).await {
            return Ok(cached);
        }

        let url = format!("{}/player?uuid={}", HYPIXEL_API_BASE, uuid);

        let response: HypixelResponse = self
            .http
            .get(&url)
            .header("API-Key", &self.key)
            .send()
            .await?
            .json()
            .await?;

        if !response.success {
            if let Some(cause) = response.cause {
                if cause.contains("limit") {
                    return Err(ClientError::RateLimited);
                }
                return Err(ClientError::HypixelApi(cause));
            }
        }

        self.cache_set(&cache_key, &response.player).await;
        Ok(response.player)
    }

    pub async fn get_guild_by_player(&self, uuid: &str) -> Result<Option<Value>, ClientError> {
        let cache_key = format!("cache:hg:player:{}", uuid.to_lowercase());

        if let Some(cached) = self.cache_get(&cache_key).await {
            return Ok(cached);
        }

        let url = format!("{}/guild?player={}", HYPIXEL_API_BASE, uuid);

        let response: HypixelResponse = self
            .http
            .get(&url)
            .header("API-Key", &self.key)
            .send()
            .await?
            .json()
            .await?;

        if !response.success {
            if let Some(cause) = response.cause {
                return Err(ClientError::HypixelApi(cause));
            }
        }

        self.cache_set(&cache_key, &response.guild).await;
        Ok(response.guild)
    }

    pub async fn get_guild_by_name(&self, name: &str) -> Result<Option<Value>, ClientError> {
        let cache_key = format!("cache:hg:name:{}", name.to_lowercase());

        if let Some(cached) = self.cache_get(&cache_key).await {
            return Ok(cached);
        }

        let url = format!("{}/guild?name={}", HYPIXEL_API_BASE, name);

        let response: HypixelResponse = self
            .http
            .get(&url)
            .header("API-Key", &self.key)
            .send()
            .await?
            .json()
            .await?;

        if !response.success {
            if let Some(cause) = response.cause {
                return Err(ClientError::HypixelApi(cause));
            }
        }

        self.cache_set(&cache_key, &response.guild).await;
        Ok(response.guild)
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
