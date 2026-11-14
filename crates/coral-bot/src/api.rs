use std::future::Future;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = "coral-bot";
const API_KEY_HEADER: &str = "X-API-Key";
const ACTOR_HEADER: &str = "X-Coral-Actor";
const COMMAND_HEADER: &str = "X-Coral-Command";

tokio::task_local! {
    static INVOCATION: Invocation;
}

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

pub struct Invocation {
    actor: u64,
    command: Option<HeaderValue>,
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
struct GuildEnvelope {
    guild: Option<serde_json::Value>,
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

    pub async fn get_guild_by_player(
        &self,
        uuid: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        let url = format!("{}/v3/hypixel/guild", self.base_url);
        let env: GuildEnvelope = self.get_query(&url, &[("player", uuid)]).await?;
        Ok(env.guild)
    }

    pub async fn get_guild_by_name(
        &self,
        name: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        let url = format!("{}/v3/hypixel/guild", self.base_url);
        let env: GuildEnvelope = self.get_query(&url, &[("name", name)]).await?;
        Ok(env.guild)
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

    async fn get_query<T: DeserializeOwned>(
        &self,
        url: &str,
        params: &[(&str, &str)],
    ) -> Result<T, ApiError> {
        self.send(self.http.get(url).query(params)).await
    }

    async fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, ApiError> {
        let response = self.authorize(request).send().await?;
        Self::parse_response(response).await
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        let request = request.header(API_KEY_HEADER, &self.api_key);
        match INVOCATION.try_with(Invocation::headers) {
            Ok(headers) => request.headers(headers),
            Err(_) => request,
        }
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

impl Invocation {
    pub fn new(actor: u64, command: &str) -> Self {
        Self {
            actor,
            command: command_header(command),
        }
    }

    pub async fn scope<F: Future>(self, work: F) -> F::Output {
        INVOCATION.scope(self, work).await
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACTOR_HEADER, HeaderValue::from(self.actor));
        if let Some(command) = &self.command {
            headers.insert(COMMAND_HEADER, command.clone());
        }
        headers
    }
}

fn command_header(command: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(command)
        .inspect_err(|e| tracing::warn!("Command {command:?} is not forwardable as a header: {e}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const ACTOR: u64 = 123_456_789_012_345_678;
    const API_KEY: &str = "internal-test-key";
    const HEAD_END: &[u8] = b"\r\n\r\n";
    const READ_CHUNK: usize = 1024;
    const GUILD_BODY: &str = r#"{"guild":null}"#;
    const LINK_BODY: &str =
        r#"{"linked":{"uuid":"0123456789abcdef0123456789abcdef","username":"Coral"}}"#;

    async fn serve_once(listener: TcpListener, body: &'static str) -> String {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut chunk = [0u8; READ_CHUNK];
        while !head
            .windows(HEAD_END.len())
            .any(|window| window == HEAD_END)
        {
            let read = stream.read(&mut chunk).await.unwrap();
            head.extend_from_slice(&chunk[..read]);
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(head).unwrap().to_ascii_lowercase()
    }

    async fn local_api(body: &'static str) -> (CoralApiClient, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(serve_once(listener, body));
        (CoralApiClient::new(base_url, API_KEY.into()), server)
    }

    #[tokio::test]
    async fn invocation_headers_reach_coral_api() {
        let (client, server) = local_api(GUILD_BODY).await;
        Invocation::new(ACTOR, "guild")
            .scope(client.get_guild_by_name("Coral"))
            .await
            .unwrap();
        let head = server.await.unwrap();
        assert!(head.contains(&format!("x-coral-actor: {ACTOR}\r\n")));
        assert!(head.contains("x-coral-command: guild\r\n"));
        assert!(head.contains("user-agent: coral-bot\r\n"));
        assert!(head.contains(&format!("x-api-key: {API_KEY}\r\n")));
    }

    #[tokio::test]
    async fn linking_forwards_the_invocation() {
        let (client, server) = local_api(LINK_BODY).await;
        Invocation::new(ACTOR, "manage_add_code_modal")
            .scope(client.link_account(ACTOR, LinkProof::Code("1234"), false))
            .await
            .unwrap();
        let head = server.await.unwrap();
        assert!(head.starts_with(&format!("post /v3/links/{ACTOR} ")));
        assert!(head.contains(&format!("x-coral-actor: {ACTOR}\r\n")));
        assert!(head.contains("x-coral-command: manage_add_code_modal\r\n"));
    }

    #[tokio::test]
    async fn requests_outside_an_invocation_carry_only_the_service() {
        let (client, server) = local_api(GUILD_BODY).await;
        client.get_guild_by_name("Coral").await.unwrap();
        let head = server.await.unwrap();
        assert!(head.contains("user-agent: coral-bot\r\n"));
        assert!(!head.contains("x-coral-actor"));
        assert!(!head.contains("x-coral-command"));
    }

    #[tokio::test]
    async fn spawned_work_runs_without_the_invocation() {
        let (client, server) = local_api(GUILD_BODY).await;
        Invocation::new(ACTOR, "guild")
            .scope(async move {
                tokio::spawn(async move { client.get_guild_by_name("Coral").await }).await
            })
            .await
            .unwrap()
            .unwrap();
        let head = server.await.unwrap();
        assert!(head.contains("user-agent: coral-bot\r\n"));
        assert!(!head.contains("x-coral-actor"));
        assert!(!head.contains("x-coral-command"));
    }

    #[test]
    fn unforwardable_command_names_keep_the_actor() {
        let headers = Invocation::new(ACTOR, "bad\nname").headers();
        assert_eq!(headers[ACTOR_HEADER], ACTOR.to_string().as_str());
        assert!(!headers.contains_key(COMMAND_HEADER));
    }
}
