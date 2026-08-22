use base64::prelude::*;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;

use crate::error::ClientError;

const MOJANG_API: &str = "https://api.mojang.com";
const SESSION_API: &str = "https://sessionserver.mojang.com";

const IDENTITY_PREFIX: &str = "mj:id:";
const UUID_PREFIX: &str = "mj:uuid:";
const VACANT_PREFIX: &str = "mj:vacant:";
const STALE_PREFIX: &str = "mj:stale:";
const PROFILE_RENDER_PREFIX: &str = "mj:profile:";
const PROFILE_METADATA_PREFIX: &str = "mj:profile-meta:";

const IDENTITY_TTL_SECS: i64 = 3 * 24 * 60 * 60;
const RENAME_LOCK_SECS: i64 = 30 * 24 * 60 * 60;
const VACANCY_SECS: i64 = 37 * 24 * 60 * 60;
const STALE_FALLBACK_SECS: u64 = 7 * 24 * 60 * 60;

const PROFILE_RENDER_TTL_SECS: u64 = 45;
const PROFILE_METADATA_TTL_SECS: u64 = IDENTITY_TTL_SECS as u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerIdentity {
    pub uuid: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub uuid: String,
    pub username: String,
    pub skin_url: Option<String>,
    pub slim: bool,
}

#[derive(Debug, Deserialize)]
struct MojangResponse {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ProfileResponse {
    id: String,
    name: String,
    properties: Vec<ProfileProperty>,
}

#[derive(Debug, Deserialize)]
struct ProfileProperty {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct TexturesPayload {
    textures: Textures,
}

#[derive(Debug, Deserialize)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<SkinTexture>,
}

#[derive(Debug, Deserialize)]
struct SkinTexture {
    url: String,
    metadata: Option<SkinMetadata>,
}

#[derive(Debug, Deserialize)]
struct SkinMetadata {
    model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrackedUsername {
    username: String,
    last_seen: i64,
}

#[derive(Clone)]
pub struct MojangClient {
    http: Client,
    redis: ConnectionManager,
}

impl MojangClient {
    pub fn new(redis: ConnectionManager) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to create HTTP client");

        Self { http, redis }
    }

    pub async fn resolve(&self, identifier: &str) -> Result<PlayerIdentity, ClientError> {
        let key = identifier.to_lowercase().replace('-', "");

        if self.is_vacant(&key).await {
            return Err(ClientError::PlayerNotFound(identifier.to_string()));
        }

        if let Some(identity) = self.get_cached::<PlayerIdentity>(&identity_key(&key)).await {
            return Ok(identity);
        }

        let result = if is_uuid(identifier) {
            self.get_profile_metadata(identifier)
                .await
                .map(|p| (normalize_uuid(&p.uuid), p.username))
        } else {
            self.fetch_identity_by_name(identifier).await
        };

        let (uuid, username) = match result {
            Ok(pair) => pair,
            Err(e) => {
                return match self.get_cached::<PlayerIdentity>(&stale_key(&key)).await {
                    Some(stale) => Ok(stale),
                    None => Err(e),
                };
            }
        };

        self.record_resolution(&key, &uuid, &username).await;

        Ok(PlayerIdentity { uuid, username })
    }

    async fn fetch_identity_by_name(&self, name: &str) -> Result<(String, String), ClientError> {
        let url = format!("{}/users/profiles/minecraft/{}", MOJANG_API, name);
        let response = self
            .http
            .get(&url)
            .header("User-Agent", "Coral/1.0 (https://urchin.ws)")
            .send()
            .await?;

        let status = response.status();
        if status == StatusCode::NO_CONTENT {
            return Err(ClientError::PlayerNotFound(name.to_string()));
        }
        if !status.is_success() {
            return Err(match status {
                StatusCode::NOT_FOUND => ClientError::PlayerNotFound(name.to_string()),
                StatusCode::TOO_MANY_REQUESTS => ClientError::RateLimited,
                other => ClientError::MojangApi(other.as_u16()),
            });
        }

        let data: MojangResponse = response.json().await?;
        Ok((normalize_uuid(&data.id), data.name))
    }

    pub async fn get_profile(&self, uuid: &str) -> Result<PlayerProfile, ClientError> {
        self.get_profile_cached(uuid, PROFILE_RENDER_PREFIX, PROFILE_RENDER_TTL_SECS)
            .await
    }

    pub async fn get_profile_metadata(&self, uuid: &str) -> Result<PlayerProfile, ClientError> {
        self.get_profile_cached(uuid, PROFILE_METADATA_PREFIX, PROFILE_METADATA_TTL_SECS)
            .await
    }

    async fn get_profile_cached(
        &self,
        uuid: &str,
        key_prefix: &str,
        ttl_secs: u64,
    ) -> Result<PlayerProfile, ClientError> {
        let key = normalize_uuid(uuid);
        let cache_key = format!("{key_prefix}{key}");

        if let Some(cached) = self.get_cached::<PlayerProfile>(&cache_key).await {
            return Ok(cached);
        }

        let profile = self.fetch_profile_uncached(uuid).await?;
        self.set_cached(&cache_key, &profile, ttl_secs).await;
        Ok(profile)
    }

    async fn fetch_profile_uncached(&self, uuid: &str) -> Result<PlayerProfile, ClientError> {
        let key = normalize_uuid(uuid);
        let url = format!("{}/session/minecraft/profile/{}", SESSION_API, key);

        let response = self
            .http
            .get(&url)
            .header("User-Agent", "Coral/1.0 (https://urchin.ws)")
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(match status {
                StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => {
                    ClientError::PlayerNotFound(uuid.to_string())
                }
                StatusCode::TOO_MANY_REQUESTS => ClientError::RateLimited,
                other => ClientError::MojangApi(other.as_u16()),
            });
        }

        let data: ProfileResponse = response.json().await?;

        let textures = data
            .properties
            .iter()
            .find(|p| p.name == "textures")
            .and_then(|p| BASE64_STANDARD.decode(&p.value).ok())
            .and_then(|bytes| serde_json::from_slice::<TexturesPayload>(&bytes).ok());

        let (skin_url, slim) = textures
            .and_then(|t| t.textures.skin)
            .map(|s| {
                let slim = s
                    .metadata
                    .as_ref()
                    .is_some_and(|m| m.model.as_deref() == Some("slim"));
                (Some(s.url), slim)
            })
            .unwrap_or((None, false));

        Ok(PlayerProfile {
            uuid: normalize_uuid(&data.id),
            username: data.name,
            skin_url,
            slim,
        })
    }

    async fn record_resolution(&self, key: &str, uuid: &str, username: &str) {
        let now = now_unix();
        let identity = PlayerIdentity {
            uuid: uuid.to_string(),
            username: username.to_string(),
        };

        self.set_cached(&stale_key(key), &identity, STALE_FALLBACK_SECS)
            .await;

        let previous: Option<TrackedUsername> = self.get_cached(&uuid_key(uuid)).await;
        let renamed = previous
            .as_ref()
            .is_some_and(|prev| prev.username.to_lowercase() != username.to_lowercase());

        if renamed {
            let prev = previous.expect("checked is_some_and");
            let name_key = username.to_lowercase();

            let lock_remaining = prev.last_seen + RENAME_LOCK_SECS - now;
            if lock_remaining > 0 {
                self.set_cached(&identity_key(&name_key), &identity, lock_remaining as u64)
                    .await;
                if key != name_key {
                    self.set_cached(&identity_key(key), &identity, lock_remaining as u64)
                        .await;
                }
            }

            let vacancy_remaining = prev.last_seen + VACANCY_SECS - now;
            if vacancy_remaining > 0 {
                self.mark_vacant(&prev.username.to_lowercase(), vacancy_remaining as u64)
                    .await;
            }
        } else {
            self.set_cached(&identity_key(key), &identity, IDENTITY_TTL_SECS as u64)
                .await;
        }

        let tracked = TrackedUsername {
            username: username.to_string(),
            last_seen: now,
        };
        self.set_cached(&uuid_key(uuid), &tracked, RENAME_LOCK_SECS as u64)
            .await;
    }

    async fn is_vacant(&self, name_key: &str) -> bool {
        self.redis
            .clone()
            .exists(vacant_key(name_key))
            .await
            .unwrap_or(false)
    }

    async fn mark_vacant(&self, name_key: &str, ttl_secs: u64) {
        if ttl_secs == 0 {
            return;
        }
        let _: Result<(), _> = self
            .redis
            .clone()
            .set_ex(vacant_key(name_key), 1u8, ttl_secs)
            .await;
    }

    async fn get_cached<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let raw: Option<String> = self.redis.clone().get(key).await.ok()?;
        raw.and_then(|s| serde_json::from_str(&s).ok())
    }

    async fn set_cached<T: Serialize>(&self, key: &str, value: &T, ttl_secs: u64) {
        if ttl_secs == 0 {
            return;
        }
        if let Ok(json) = serde_json::to_string(value) {
            let _: Result<(), _> = self.redis.clone().set_ex(key, json, ttl_secs).await;
        }
    }
}

fn identity_key(key: &str) -> String {
    format!("{IDENTITY_PREFIX}{key}")
}

fn uuid_key(uuid: &str) -> String {
    format!("{UUID_PREFIX}{uuid}")
}

fn vacant_key(name_key: &str) -> String {
    format!("{VACANT_PREFIX}{name_key}")
}

fn stale_key(key: &str) -> String {
    format!("{STALE_PREFIX}{key}")
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn is_uuid(s: &str) -> bool {
    let stripped = s.replace('-', "");
    stripped.len() == 32 && stripped.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn normalize_uuid(uuid: &str) -> String {
    uuid.replace('-', "").to_lowercase()
}
