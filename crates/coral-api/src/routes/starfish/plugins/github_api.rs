use serde::Deserialize;

use crate::error::ApiError;


const GITHUB_API: &str = "https://api.github.com";


#[derive(Debug, Clone, Deserialize)]
pub struct Repo {
    pub id: i64,
    pub full_name: String,
    pub default_branch: String,
    pub html_url: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct PermissionResponse {
    pub permission: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub target_commitish: String,
    pub assets: Vec<Asset>,
    pub body: Option<String>,
}


#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub id: i64,
    pub name: String,
    pub size: i32,
    pub url: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub digest: Option<String>,
}


pub async fn get_user(access_token: &str) -> Result<(i64, String), ApiError> {
    #[derive(Deserialize)] struct User { id: i64, login: String }
    let user: User = request(access_token, &format!("{GITHUB_API}/user")).await?;
    Ok((user.id, user.login))
}


pub async fn get_repo(access_token: &str, repo: &str) -> Result<Repo, ApiError> {
    request(access_token, &format!("{GITHUB_API}/repos/{repo}")).await
}


pub async fn get_permission(access_token: &str, repo: &str, username: &str) -> Result<String, ApiError> {
    let resp: PermissionResponse = request(
        access_token,
        &format!("{GITHUB_API}/repos/{repo}/collaborators/{username}/permission"),
    ).await?;
    Ok(resp.permission)
}


pub async fn get_release_by_tag(access_token: &str, repo: &str, tag: &str) -> Result<Release, ApiError> {
    request(access_token, &format!("{GITHUB_API}/repos/{repo}/releases/tags/{tag}")).await
}


pub async fn download_asset(access_token: &str, asset: &Asset) -> Result<Vec<u8>, ApiError> {
    let response = reqwest::Client::new()
        .get(&asset.url)
        .bearer_auth(access_token)
        .header("Accept", "application/octet-stream")
        .header("User-Agent", "coral-api")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("GitHub asset fetch: {e}")))?;

    if !response.status().is_success() {
        return Err(ApiError::ExternalApi(format!(
            "GitHub returned {} for asset {}", response.status(), asset.name
        )));
    }

    response.bytes().await
        .map(|b| b.to_vec())
        .map_err(|e| ApiError::ExternalApi(format!("asset download: {e}")))
}


async fn request<T: for<'de> Deserialize<'de>>(access_token: &str, url: &str) -> Result<T, ApiError> {
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(access_token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "coral-api")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("GitHub API: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 => ApiError::Unauthorized("invalid_github_token".into()),
            403 => ApiError::Forbidden("github_access_denied".into()),
            404 => ApiError::NotFound(format!("GitHub resource not found: {url}")),
            _ => ApiError::ExternalApi(format!("GitHub returned {status}")),
        });
    }

    response.json().await
        .map_err(|e| ApiError::ExternalApi(format!("parse GitHub response: {e}")))
}
