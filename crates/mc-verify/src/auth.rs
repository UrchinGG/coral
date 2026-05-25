use serde::Deserialize;
use uuid::Uuid;

const SESSION_URL: &str = "https://sessionserver.mojang.com/session/minecraft/hasJoined";

#[derive(Debug, Deserialize)]
struct AuthResponse {
    id: String,
    name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("mojang rejected the session")]
    Rejected,
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid uuid from mojang: {0}")]
    InvalidUuid(String),
}

pub struct AuthedPlayer {
    pub uuid: Uuid,
    pub username: String,
}

pub async fn verify_session(
    http: &reqwest::Client,
    username: &str,
    server_hash: &str,
) -> Result<AuthedPlayer, AuthError> {
    let resp = http
        .get(SESSION_URL)
        .query(&[("username", username), ("serverId", server_hash)])
        .send()
        .await?;
    if resp.status() != 200 {
        return Err(AuthError::Rejected);
    }
    let auth: AuthResponse = resp.json().await?;
    let uuid =
        parse_undashed_uuid(&auth.id).ok_or_else(|| AuthError::InvalidUuid(auth.id.clone()))?;
    Ok(AuthedPlayer {
        uuid,
        username: auth.name,
    })
}

fn parse_undashed_uuid(s: &str) -> Option<Uuid> {
    if s.len() != 32 {
        return None;
    }
    Uuid::parse_str(&format!(
        "{}-{}-{}-{}-{}",
        &s[..8],
        &s[8..12],
        &s[12..16],
        &s[16..20],
        &s[20..]
    ))
    .ok()
}
