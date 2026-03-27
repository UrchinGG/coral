use std::time::Duration;

use moka::future::Cache;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::error::ClientError;

const HYPIXEL_API_BASE: &str = "https://api.hypixel.net/v2";
const CACHE_TTL: Duration = Duration::from_secs(30);


#[derive(Clone)]
pub struct HypixelClient {
    http: Client,
    key: String,
    player_cache: Cache<String, Option<Value>>,
    guild_cache: Cache<String, Option<Value>>,
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
    pub fn new(key: String) -> Result<Self, ClientError> {
        if key.is_empty() {
            return Err(ClientError::NoApiKeys);
        }

        let player_cache = Cache::builder()
            .time_to_live(CACHE_TTL)
            .max_capacity(5_000)
            .build();

        let guild_cache = Cache::builder()
            .time_to_live(CACHE_TTL)
            .max_capacity(5_000)
            .build();

        Ok(Self { http: Client::new(), key, player_cache, guild_cache })
    }

    pub async fn get_player(&self, uuid: &str) -> Result<Option<Value>, ClientError> {
        let cache_key = uuid.to_lowercase();

        if let Some(cached) = self.player_cache.get(&cache_key).await {
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

        self.player_cache
            .insert(cache_key, response.player.clone())
            .await;

        Ok(response.player)
    }

    pub async fn get_guild_by_player(&self, uuid: &str) -> Result<Option<Value>, ClientError> {
        let cache_key = format!("player:{}", uuid.to_lowercase());

        if let Some(cached) = self.guild_cache.get(&cache_key).await {
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

        self.guild_cache
            .insert(cache_key, response.guild.clone())
            .await;

        Ok(response.guild)
    }

    pub async fn get_guild_by_name(&self, name: &str) -> Result<Option<Value>, ClientError> {
        let cache_key = format!("name:{}", name.to_lowercase());

        if let Some(cached) = self.guild_cache.get(&cache_key).await {
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

        self.guild_cache
            .insert(cache_key, response.guild.clone())
            .await;

        Ok(response.guild)
    }
}
