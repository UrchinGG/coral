use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::prelude::*;
use moka::future::Cache;
use reqwest::Client;
use serde::Deserialize;

use crate::error::ClientError;

const MOJANG_API: &str = "https://api.mojang.com";
const SESSION_API: &str = "https://sessionserver.mojang.com";

const CACHE_TTL_DEFAULT: Duration = Duration::from_secs(30);
const CACHE_TTL_NAME_LOCKED: Duration = Duration::from_secs(30 * 24 * 60 * 60); // 30 days

#[derive(Debug, Clone)]
pub struct PlayerIdentity {
    pub uuid: String,
    pub username: String,
}

#[derive(Debug, Clone)]
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

struct CachedIdentity {
    identity: PlayerIdentity,
    cached_at: Instant,
    name_locked_until: Option<Instant>,
}

struct TrackedUsername {
    username: String,
    last_seen: Instant,
}

pub struct MojangClient {
    http: Client,
    identity_cache: Cache<String, Arc<CachedIdentity>>,
    uuid_to_username: Cache<String, Arc<TrackedUsername>>,
    profile_cache: Cache<String, Arc<PlayerProfile>>,
}

impl MojangClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to create HTTP client");

        let identity_cache = Cache::builder()
            .time_to_live(CACHE_TTL_NAME_LOCKED)
            .max_capacity(10_000)
            .build();

        let uuid_to_username = Cache::builder()
            .time_to_live(CACHE_TTL_NAME_LOCKED)
            .max_capacity(10_000)
            .build();

        let profile_cache = Cache::builder()
            .time_to_live(CACHE_TTL_DEFAULT)
            .max_capacity(5_000)
            .build();

        Self {
            http,
            identity_cache,
            uuid_to_username,
            profile_cache,
        }
    }

    pub async fn resolve(&self, identifier: &str) -> Result<PlayerIdentity, ClientError> {
        let key = identifier.to_lowercase().replace('-', "");

        let stale = self.identity_cache.get(&key).await;

        if let Some(ref cached) = stale {
            let now = Instant::now();

            if cached
                .name_locked_until
                .is_some_and(|locked_until| now < locked_until)
            {
                return Ok(cached.identity.clone());
            }

            if now.duration_since(cached.cached_at) < CACHE_TTL_DEFAULT {
                return Ok(cached.identity.clone());
            }
        }

        let result = if is_uuid(identifier) {
            self.get_profile(identifier)
                .await
                .map(|p| (normalize_uuid(&p.uuid), p.username))
        } else {
            self.fetch_identity_by_name(identifier).await
        };

        let (uuid, username) = match result {
            Ok(pair) => pair,
            Err(_) if stale.is_some() => {
                return Ok(stale.expect("checked is_some").identity.clone());
            }
            Err(e) => return Err(e),
        };

        let now = Instant::now();

        let name_locked_until = self
            .uuid_to_username
            .get(&uuid)
            .await
            .filter(|old| old.username.to_lowercase() != key)
            .filter(|old| now.duration_since(old.last_seen) < CACHE_TTL_NAME_LOCKED)
            .map(|old| old.last_seen + CACHE_TTL_NAME_LOCKED);

        let identity = PlayerIdentity {
            uuid: uuid.clone(),
            username: username.clone(),
        };

        let cached = Arc::new(CachedIdentity {
            identity: identity.clone(),
            cached_at: now,
            name_locked_until,
        });

        self.identity_cache.insert(key, cached).await;
        self.uuid_to_username
            .insert(
                uuid,
                Arc::new(TrackedUsername {
                    username,
                    last_seen: now,
                }),
            )
            .await;

        Ok(identity)
    }

    async fn fetch_identity_by_name(
        &self,
        name: &str,
    ) -> Result<(String, String), ClientError> {
        let url = format!("{}/users/profiles/minecraft/{}", MOJANG_API, name);
        let response = self
            .http
            .get(&url)
            .header("User-Agent", "Coral/1.0 (https://urchin.ws)")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ClientError::PlayerNotFound(name.to_string()));
        }

        let data: MojangResponse = response.json().await?;
        Ok((normalize_uuid(&data.id), data.name))
    }

    pub async fn get_profile(&self, uuid: &str) -> Result<PlayerProfile, ClientError> {
        let key = normalize_uuid(uuid);

        if let Some(cached) = self.profile_cache.get(&key).await {
            return Ok((*cached).clone());
        }

        let url = format!("{}/session/minecraft/profile/{}", SESSION_API, key);

        let response = self
            .http
            .get(&url)
            .header("User-Agent", "Coral/1.0 (https://urchin.ws)")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ClientError::PlayerNotFound(uuid.to_string()));
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

        let profile = PlayerProfile {
            uuid: normalize_uuid(&data.id),
            username: data.name,
            skin_url,
            slim,
        };

        self.profile_cache
            .insert(key, Arc::new(profile.clone()))
            .await;

        Ok(profile)
    }
}

impl Default for MojangClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MojangClient {
    fn clone(&self) -> Self {
        Self {
            http: self.http.clone(),
            identity_cache: self.identity_cache.clone(),
            uuid_to_username: self.uuid_to_username.clone(),
            profile_cache: self.profile_cache.clone(),
        }
    }
}

pub fn is_uuid(s: &str) -> bool {
    let stripped = s.replace('-', "");
    stripped.len() == 32 && stripped.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn normalize_uuid(uuid: &str) -> String {
    uuid.replace('-', "").to_lowercase()
}
