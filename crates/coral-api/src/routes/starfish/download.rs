use std::collections::HashMap;

use axum::{Json, Router, body::Body, extract::{Query, State}, http::{StatusCode, header}, response::Response, routing::get};
use serde::{Deserialize, Serialize};

use database::StarfishRepository;

use crate::error::ApiError;

use super::{auth::fetch_discord_user, require_starfish};
use crate::state::{AppState, StarfishConfig};


const GITHUB_API_URL: &str = "https://api.github.com";


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/download/info", get(get_release_info))
        .route("/download/latest", get(download_latest))
}



#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Linux,
    Macos,
}

impl Platform {
    fn matches(&self, filename: &str) -> bool {
        match self {
            Self::Windows => filename.ends_with(".exe"),
            Self::Linux => filename.ends_with(".AppImage") || filename.ends_with(".tar.gz"),
            Self::Macos => filename.ends_with(".dmg") || filename.ends_with(".app.zip"),
        }
    }
}

#[derive(Serialize)]
pub struct PlatformAsset {
    pub filename: String,
    pub size: u64,
}

#[derive(Serialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub name: String,
    pub published_at: String,
    pub platforms: HashMap<String, PlatformAsset>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    published_at: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    size: u64,
    url: String,
}



async fn get_release_info(
    State(state): State<AppState>,
) -> Result<Json<ReleaseInfo>, ApiError> {
    let config = require_starfish(&state)?;
    let release = fetch_latest_release(&config).await?;

    let mut platforms = HashMap::new();
    for (platform, key) in [(Platform::Windows, "windows"), (Platform::Linux, "linux"), (Platform::Macos, "macos")] {
        if let Some(asset) = release.assets.iter().find(|a| platform.matches(&a.name)) {
            platforms.insert(key.to_string(), PlatformAsset { filename: asset.name.clone(), size: asset.size });
        }
    }

    Ok(Json(ReleaseInfo {
        version: release.tag_name,
        name: release.name.unwrap_or_default(),
        published_at: release.published_at,
        platforms,
    }))
}



#[derive(Deserialize)]
pub struct DownloadQuery {
    pub token: Option<String>,
    pub platform: Option<Platform>,
}

async fn download_latest(
    State(state): State<AppState>,
    Query(query): Query<DownloadQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let config = require_starfish(&state)?;

    let discord_token = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or(query.token.as_deref())
        .ok_or_else(|| ApiError::Unauthorized("Missing authorization".into()))?;

    let discord_user = fetch_discord_user(discord_token).await?;
    let discord_id: i64 = discord_user.id.parse()
        .map_err(|_| ApiError::Internal("Invalid Discord ID".into()))?;

    let repo = StarfishRepository::new(state.db.pool());
    let user = repo.get_user_by_discord_id(discord_id).await?;
    match user.as_ref().map(|u| u.license_status.as_str()) {
        Some("active") => {}
        Some(_) => return Err(ApiError::Unauthorized("License required".into())),
        None => return Err(ApiError::Unauthorized("User not registered".into())),
    }

    let platform = query.platform.unwrap_or(Platform::Windows);
    let release = fetch_latest_release(&config).await?;

    let asset = release.assets.iter()
        .find(|a| platform.matches(&a.name))
        .ok_or_else(|| ApiError::NotFound(format!("No {platform:?} asset in release")))?;

    let response = reqwest::Client::new()
        .get(&asset.url)
        .bearer_auth(&config.github_token)
        .header("Accept", "application/octet-stream")
        .header("User-Agent", "coral-api")
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("GitHub API error: {e}")))?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::FOUND {
        return Err(ApiError::ExternalApi("Failed to fetch release from GitHub".into()));
    }

    let bytes = response.bytes().await
        .map_err(|e| ApiError::ExternalApi(format!("Failed to download release: {e}")))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", asset.name))
        .header(header::CONTENT_LENGTH, bytes.len())
        .body(Body::from(bytes.to_vec()))
        .map_err(|e| ApiError::Internal(format!("Failed to build response: {e}")))?)
}



async fn fetch_latest_release(config: &StarfishConfig) -> Result<GitHubRelease, ApiError> {
    let url = format!("{GITHUB_API_URL}/repos/{}/releases", config.github_repo);

    let releases: Vec<GitHubRelease> = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&config.github_token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "coral-api")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("GitHub API error: {e}")))?
        .json().await
        .map_err(|e| ApiError::ExternalApi(format!("Failed to parse GitHub response: {e}")))?;

    releases.into_iter().next()
        .ok_or_else(|| ApiError::NotFound("No releases found".into()))
}
