use std::collections::HashMap;

use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{StatusCode, header},
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiError,
    state::{AppState, StarfishConfig},
};

use super::{require_starfish, session_auth};

const GITHUB_API_URL: &str = "https://api.github.com";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/download/info", get(get_release_info))
        .route("/download/releases", get(list_releases))
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
    /// Every platform ships one directly-executable file with a fixed name, so the
    /// updater can `rename` it straight over the running binary — no installers or
    /// archives to unpack.
    fn binary_filename(&self) -> &'static str {
        match self {
            Self::Windows => "starfish-windows.exe",
            Self::Linux => "starfish-linux",
            Self::Macos => "starfish-macos",
        }
    }

    fn matches_binary(&self, filename: &str) -> bool {
        filename == self.binary_filename()
    }

    fn matches_signature(&self, binary_name: &str, filename: &str) -> bool {
        filename == format!("{binary_name}.sig")
    }
}

#[derive(Serialize)]
pub struct PlatformAsset {
    pub filename: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_filename: Option<String>,
}

#[derive(Serialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub name: String,
    pub published_at: String,
    pub release_notes: Option<String>,
    pub prerelease: bool,
    pub platforms: HashMap<String, PlatformAsset>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    published_at: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    size: u64,
    url: String,
}

async fn get_release_info(State(state): State<AppState>) -> Result<Json<ReleaseInfo>, ApiError> {
    let config = require_starfish(&state)?;
    let release = fetch_latest_release(&config).await?;
    Ok(Json(to_release_info(release)))
}

async fn list_releases(State(state): State<AppState>) -> Result<Json<Vec<ReleaseInfo>>, ApiError> {
    let config = require_starfish(&state)?;
    let releases = fetch_releases(&config).await?
        .into_iter()
        .filter(is_public_release)
        .map(to_release_info)
        .collect();
    Ok(Json(releases))
}

fn to_release_info(release: GitHubRelease) -> ReleaseInfo {
    let mut platforms = HashMap::new();
    for (platform, key) in [
        (Platform::Windows, "windows"),
        (Platform::Linux, "linux"),
        (Platform::Macos, "macos"),
    ] {
        if let Some(asset) = release
            .assets
            .iter()
            .find(|a| platform.matches_binary(&a.name))
        {
            let signature_filename = release
                .assets
                .iter()
                .find(|a| platform.matches_signature(&asset.name, &a.name))
                .map(|a| a.name.clone());
            platforms.insert(
                key.to_string(),
                PlatformAsset {
                    filename: asset.name.clone(),
                    size: asset.size,
                    signature_filename,
                },
            );
        }
    }

    ReleaseInfo {
        version: release.tag_name,
        name: release.name.unwrap_or_default(),
        published_at: release.published_at,
        release_notes: release.body,
        prerelease: release.prerelease,
        platforms,
    }
}

fn is_public_release(release: &GitHubRelease) -> bool {
    !release.draft && has_all_platform_assets(release)
}

fn has_all_platform_assets(release: &GitHubRelease) -> bool {
    [Platform::Windows, Platform::Linux, Platform::Macos].into_iter().all(|platform| {
        let Some(binary) = release.assets.iter().find(|a| platform.matches_binary(&a.name)) else {
            return false;
        };
        release.assets.iter().any(|a| platform.matches_signature(&binary.name, &a.name))
    })
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    pub token: Option<String>,
    pub platform: Option<Platform>,
    pub asset: Option<String>,
}

async fn download_latest(
    State(state): State<AppState>,
    Query(query): Query<DownloadQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let config = require_starfish(&state)?;
    authorize_download(&state, &headers, &query).await?;

    let platform = query.platform.unwrap_or(Platform::Windows);
    let release = fetch_latest_release(&config).await?;

    let requested_signature = query.asset.as_deref() == Some("signature");

    let asset = if requested_signature {
        let binary = release
            .assets
            .iter()
            .find(|a| platform.matches_binary(&a.name))
            .ok_or_else(|| ApiError::NotFound(format!("No {platform:?} asset in release")))?;
        release
            .assets
            .iter()
            .find(|a| platform.matches_signature(&binary.name, &a.name))
            .ok_or_else(|| ApiError::NotFound(format!("No {platform:?} signature in release")))?
    } else {
        release
            .assets
            .iter()
            .find(|a| platform.matches_binary(&a.name))
            .ok_or_else(|| ApiError::NotFound(format!("No {platform:?} asset in release")))?
    };

    let response = reqwest::Client::new()
        .get(&asset.url)
        .bearer_auth(&config.github_token)
        .header("Accept", "application/octet-stream")
        .header("User-Agent", "coral-api")
        .send()
        .await
        .map_err(|e| ApiError::ExternalApi(format!("GitHub API error: {e}")))?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::FOUND {
        return Err(ApiError::ExternalApi(
            "Failed to fetch release from GitHub".into(),
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ApiError::ExternalApi(format!("Failed to download release: {e}")))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", asset.name),
        )
        .header(header::CONTENT_LENGTH, bytes.len())
        .body(Body::from(bytes.to_vec()))
        .map_err(|e| ApiError::Internal(format!("Failed to build response: {e}")))?)
}

/// The desktop app authenticates with its starfish session headers; the website keeps
/// using a Discord OAuth bearer token. Both paths require an active license.
async fn authorize_download(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    query: &DownloadQuery,
) -> Result<(), ApiError> {
    if let Some((token, hwid, signature)) = starfish_session_headers(headers) {
        super::auth::validate_hwid(&hwid)?;
        let repo = database::StarfishRepository::new(state.db.pool());
        session_auth::resolve_verified_session(&repo, &token, &hwid, &signature).await?;
        return Ok(());
    }

    let discord_token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or(query.token.as_deref())
        .ok_or_else(|| ApiError::Unauthorized("Missing authorization".into()))?;

    let user = super::resolve_discord_user(state, discord_token).await?;
    match user.as_ref().map(|u| u.license_status.as_str()) {
        Some("active") => Ok(()),
        Some(_) => Err(ApiError::Unauthorized("License required".into())),
        None => Err(ApiError::Unauthorized("User not registered".into())),
    }
}

fn starfish_session_headers(headers: &axum::http::HeaderMap) -> Option<(String, String, String)> {
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    };
    Some((
        get("X-Starfish-Session")?,
        get("X-Starfish-HWID")?,
        get("X-Starfish-Signature")?,
    ))
}

async fn fetch_latest_release(config: &StarfishConfig) -> Result<GitHubRelease, ApiError> {
    fetch_releases(config).await?
        .into_iter()
        .find(is_public_release)
        .ok_or_else(|| ApiError::NotFound("No releases found".into()))
}

async fn fetch_releases(config: &StarfishConfig) -> Result<Vec<GitHubRelease>, ApiError> {
    let url = format!("{GITHUB_API_URL}/repos/{}/releases", config.github_repo);

    reqwest::Client::new()
        .get(&url)
        .bearer_auth(&config.github_token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "coral-api")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| ApiError::ExternalApi(format!("GitHub API error: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::ExternalApi(format!("Failed to parse GitHub response: {e}")))
}
