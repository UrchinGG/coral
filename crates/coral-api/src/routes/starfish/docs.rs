use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{
    error::ApiError,
    state::{AppState, StarfishConfig},
};

use super::{
    download::{GITHUB_API_URL, fetch_latest_release},
    require_starfish,
};

const REFERENCE_PATH: &str = "docs/plugin-reference.json";
const TEMPLATE_PATH: &str = "plugins/template/plugin.lua";

pub fn router() -> Router<AppState> {
    Router::new().route("/docs", get(get_docs))
}

#[derive(Serialize)]
struct StarfishDocs {
    version: String,
    template: String,
    #[serde(flatten)]
    reference: Map<String, Value>,
}

async fn get_docs(State(state): State<AppState>) -> Result<Json<StarfishDocs>, ApiError> {
    let config = require_starfish(&state)?;
    let version = fetch_latest_release(&config).await?.tag_name;

    let (reference, template) = tokio::try_join!(
        fetch_file(&config, &version, REFERENCE_PATH),
        fetch_file(&config, &version, TEMPLATE_PATH),
    )?;
    let reference = serde_json::from_str(&reference)
        .map_err(|e| ApiError::Upstream(format!("Invalid {REFERENCE_PATH} in {version}: {e}")))?;

    Ok(Json(StarfishDocs {
        version,
        template,
        reference,
    }))
}

async fn fetch_file(config: &StarfishConfig, tag: &str, path: &str) -> Result<String, ApiError> {
    let url = format!(
        "{GITHUB_API_URL}/repos/{}/contents/{path}?ref={tag}",
        config.github_repo
    );

    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&config.github_token)
        .header("Accept", "application/vnd.github.raw+json")
        .header("User-Agent", "coral-api")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("GitHub API error: {e}")))?;

    match response.status() {
        status if status.is_success() => response
            .text()
            .await
            .map_err(|e| ApiError::Upstream(format!("Failed to read {path}: {e}"))),
        reqwest::StatusCode::NOT_FOUND => Err(ApiError::NotFound(format!("No {path} in {tag}"))),
        status => Err(ApiError::Upstream(format!(
            "GitHub API returned {status} for {path}"
        ))),
    }
}
