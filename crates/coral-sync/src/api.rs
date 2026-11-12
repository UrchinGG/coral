use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = "coral-sync";

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("HTTP {0}: {1}")]
    Http(u16, String),
    #[error("{0}")]
    Network(#[from] reqwest::Error),
}

pub struct CoralApiClient {
    http: Client,
    base_url: String,
    api_key: String,
}

#[derive(Deserialize)]
pub struct PlayerStatsResponse {
    pub uuid: String,
    pub username: String,
    pub hypixel: Option<serde_json::Value>,
    #[serde(default)]
    pub tags: Vec<TagInfo>,
    pub skin_url: Option<String>,
    #[serde(default)]
    pub slim: bool,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Deserialize)]
pub struct TagInfo {
    pub tag_type: String,
}

#[derive(Deserialize)]
pub struct ResolveResponse {
    pub uuid: String,
    pub username: String,
}

#[derive(Deserialize)]
pub struct LinkedAccount {
    pub uuid: String,
    pub username: Option<String>,
}

#[derive(Deserialize)]
pub struct Links {
    pub primary: Option<LinkedAccount>,
    pub alts: Vec<LinkedAccount>,
}

#[derive(Deserialize)]
pub struct LinkMethods {
    pub suggested: Vec<LinkedAccount>,
}

#[derive(Deserialize)]
pub struct LinkResult {
    pub linked: LinkedAccount,
}

pub enum LinkProof<'a> {
    Code(&'a str),
    Social(&'a str),
    Force(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRefusal {
    AlreadyLinked,
    NotFound,
    Unverified,
    Unconfirmed,
    TooManyAttempts,
}

impl CoralApiClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to create HTTP client");
        Self {
            http,
            base_url,
            api_key,
        }
    }

    pub async fn get_player_stats(
        &self,
        identifier: &str,
    ) -> Result<PlayerStatsResponse, ApiError> {
        let param = if identifier.len() == 32 && identifier.chars().all(|c| c.is_ascii_hexdigit()) {
            "uuid"
        } else {
            "name"
        };
        self.get(&format!(
            "{}/v3/player/profile?{}={}",
            self.base_url, param, identifier
        ))
        .await
    }

    pub async fn resolve(&self, identifier: &str) -> Result<ResolveResponse, ApiError> {
        self.get(&format!("{}/v3/resolve/{}", self.base_url, identifier))
            .await
    }

    pub async fn links(&self, discord_id: u64) -> Result<Links, ApiError> {
        self.send(self.http.get(self.links_url(discord_id, "")))
            .await
    }

    pub async fn link_methods(&self, discord_id: u64) -> Result<LinkMethods, ApiError> {
        self.send(self.http.get(self.links_url(discord_id, "/methods")))
            .await
    }

    pub async fn link_account(
        &self,
        discord_id: u64,
        proof: LinkProof<'_>,
        primary: bool,
    ) -> Result<LinkResult, ApiError> {
        let request = self
            .http
            .post(self.links_url(discord_id, ""))
            .json(&proof.body(primary));
        self.send(request).await
    }

    pub async fn unlink_account(&self, discord_id: u64, uuid: &str) -> Result<Links, ApiError> {
        let url = self.links_url(discord_id, &format!("/accounts/{uuid}"));
        self.send(self.http.delete(url)).await
    }

    pub async fn set_primary(&self, discord_id: u64, uuid: &str) -> Result<Links, ApiError> {
        let request = self
            .http
            .put(self.links_url(discord_id, "/primary"))
            .json(&serde_json::json!({ "uuid": uuid }));
        self.send(request).await
    }

    fn links_url(&self, discord_id: u64, rest: &str) -> String {
        format!("{}/v3/links/{discord_id}{rest}", self.base_url)
    }

    async fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T, ApiError> {
        self.send(self.http.get(url)).await
    }

    async fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, ApiError> {
        let response = request.header("X-API-Key", &self.api_key).send().await?;
        Self::parse_response(response).await
    }

    async fn parse_response<T: DeserializeOwned>(response: Response) -> Result<T, ApiError> {
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return match status {
                StatusCode::NOT_FOUND => Err(ApiError::NotFound),
                _ => Err(ApiError::Http(status.as_u16(), body)),
            };
        }
        Ok(response.json().await?)
    }
}

impl ApiError {
    /// Why Coral refused a link, as opposed to a failure to reach it.
    pub fn link_refusal(&self) -> Option<LinkRefusal> {
        match self {
            Self::NotFound => Some(LinkRefusal::NotFound),
            Self::Http(403, _) => Some(LinkRefusal::Unverified),
            Self::Http(409, _) => Some(LinkRefusal::AlreadyLinked),
            Self::Http(429, _) => Some(LinkRefusal::TooManyAttempts),
            Self::Http(503, _) => Some(LinkRefusal::Unconfirmed),
            _ => None,
        }
    }
}

impl LinkProof<'_> {
    fn body(&self, primary: bool) -> serde_json::Value {
        let (method, field, value) = match self {
            Self::Code(code) => ("code", "code", code),
            Self::Social(player) => ("social", "player", player),
            Self::Force(player) => ("force", "player", player),
        };
        serde_json::json!({ "method": method, (field): value, "primary": primary })
    }
}
