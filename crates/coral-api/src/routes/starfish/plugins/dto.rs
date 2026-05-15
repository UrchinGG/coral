use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize)]
pub struct PluginListResponse {
    pub total: i64,
    pub plugins: Vec<PluginSummaryDto>,
}


#[derive(Debug, Serialize)]
pub struct PluginSummaryDto {
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub official: bool,
    pub tags: Vec<String>,
    pub latest_version: String,
    pub updated_at: DateTime<Utc>,
    pub installs_30d: i64,
    pub installs_total: i64,
    pub rating_mean: Option<f32>,
    pub rating_count: i64,
    pub rating_bayesian: f32,
}


#[derive(Debug, Serialize)]
pub struct PluginDetailDto {
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub official: bool,
    pub unlisted: bool,
    pub tags: Vec<String>,
    pub license: String,
    pub homepage: Option<String>,
    pub repo_url: String,
    pub latest_release: ReleaseInfoDto,
    pub releases: Vec<ReleaseInfoDto>,
    pub readme: Option<String>,
    pub installs_30d: i64,
    pub installs_total: i64,
    pub rating_mean: Option<f32>,
    pub rating_count: i64,
    pub rating_bayesian: f32,
    pub user_rating: Option<i16>,
    pub is_installed: bool,
    pub installed_version: Option<String>,
}


#[derive(Debug, Clone, Serialize)]
pub struct ReleaseInfoDto {
    pub version: String,
    pub git_sha: String,
    pub changelog: Option<String>,
    pub asset_sha256: String,
    pub asset_size: i32,
    pub yanked: bool,
    pub yanked_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}


#[derive(Debug, Deserialize)]
pub struct PluginListQuery {
    pub sort: Option<String>,
    pub tag: Option<String>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub official: Option<bool>,
}


#[derive(Debug, Deserialize)]
pub struct DisabledQuery {
    pub since: Option<DateTime<Utc>>,
}


#[derive(Debug, Serialize)]
pub struct DisabledResponse {
    pub as_of: DateTime<Utc>,
    pub disabled: Vec<DisabledEntryDto>,
}


#[derive(Debug, Serialize)]
pub struct DisabledEntryDto {
    pub slug: String,
    pub reason: String,
    pub disabled_at: DateTime<Utc>,
}


#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    pub repo: String,
    pub version: String,
    pub release_tag: String,
    pub github_access_token: String,
}


#[derive(Debug, Serialize)]
pub struct PublishResponse {
    pub slug: String,
    pub version: String,
    pub asset_sha256: String,
    pub published_at: DateTime<Utc>,
}


#[derive(Debug, Deserialize)]
pub struct BodyQuery {
    pub version: Option<String>,
}


#[derive(Debug, Serialize)]
pub struct InstallResponse {
    pub slug: String,
    pub version: String,
    pub asset_sha256: String,
    pub asset_size: i32,
    pub manifest: serde_json::Value,
    pub body_url: String,
}


#[derive(Debug, Deserialize)]
pub struct RateRequest {
    pub stars: i16,
    pub review: Option<String>,
}


#[derive(Debug, Serialize)]
pub struct InstalledResponse {
    pub installs: Vec<InstalledEntryDto>,
}


#[derive(Debug, Serialize)]
pub struct InstalledEntryDto {
    pub slug: String,
    pub installed_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub disabled: bool,
    pub latest_release: ReleaseInfoDto,
}
