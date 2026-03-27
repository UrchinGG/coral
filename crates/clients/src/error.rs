use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Hypixel API error: {0}")]
    HypixelApi(String),

    #[error("Player not found: {0}")]
    PlayerNotFound(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),

    #[error("No API keys configured")]
    NoApiKeys,
}
