use axum::{Extension, Json, Router, extract::State, middleware, routing::{get, post}};
use serde::{Deserialize, Serialize};

use database::StarfishRepository;

use crate::{error::ApiError, state::AppState};

use super::is_owner;
use super::session_auth::{AuthenticatedStarfishUser, require_starfish_session};


const GITHUB_USER_URL: &str = "https://api.github.com/user";


pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/users/me", get(get_me))
        .route("/users/me/link-github", post(link_github))
        .route_layer(middleware::from_fn_with_state(state, require_starfish_session))
}


#[derive(Serialize)]
struct MeResponse {
    discord_id: i64,
    github_username: Option<String>,
    is_owner: bool,
}


async fn get_me(
    Extension(caller): Extension<AuthenticatedStarfishUser>,
) -> Result<Json<MeResponse>, ApiError> {
    Ok(Json(MeResponse {
        discord_id: caller.user.discord_id,
        github_username: caller.user.github_username.clone(),
        is_owner: is_owner(caller.user.discord_id),
    }))
}


#[derive(Deserialize)]
struct LinkGitHubRequest {
    access_token: String,
}

#[derive(Serialize)]
struct LinkGitHubResponse {
    github_user_id: i64,
    github_username: String,
}

#[derive(Deserialize)]
struct GitHubUser {
    id: i64,
    login: String,
}


async fn link_github(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Json(req): Json<LinkGitHubRequest>,
) -> Result<Json<LinkGitHubResponse>, ApiError> {
    let github_user = fetch_github_user(&req.access_token).await?;

    let repo = StarfishRepository::new(state.db.pool());
    repo.link_github(caller.user.id, github_user.id, &github_user.login).await?;

    Ok(Json(LinkGitHubResponse {
        github_user_id: github_user.id,
        github_username: github_user.login,
    }))
}


pub(crate) async fn fetch_github_user(access_token: &str) -> Result<GitHubUser, ApiError> {
    let response = reqwest::Client::new()
        .get(GITHUB_USER_URL)
        .bearer_auth(access_token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "coral-api")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("GitHub API error: {e}")))?;

    if !response.status().is_success() {
        return Err(ApiError::Unauthorized("invalid_github_token".into()));
    }

    response.json().await
        .map_err(|e| ApiError::ExternalApi(format!("Failed to parse GitHub response: {e}")))
}
