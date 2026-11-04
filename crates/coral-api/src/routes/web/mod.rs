pub(crate) mod session;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{AppendHeaders, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, CorsLayer};

use database::{AccessRank, MemberRepository, StarfishRepository};

use crate::{
    error::ApiError,
    routes::starfish::require_starfish,
    state::{AppState, StarfishConfig},
};

use coral_redis::web_session::{self, WebUser};

const CALLBACK: &str = "https://api.urchin.gg/auth/discord/callback";
const DISCORD_AUTHORIZE: &str = "https://discord.com/oauth2/authorize";
const DISCORD_TOKEN: &str = "https://discord.com/api/v10/oauth2/token";
const DISCORD_USER: &str = "https://discord.com/api/v10/users/@me";

#[derive(Deserialize)]
struct SignInQuery {
    #[serde(rename = "return")]
    return_to: Option<String>,
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
struct DiscordToken {
    access_token: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    global_name: Option<String>,
    avatar: Option<String>,
}

#[derive(Serialize)]
struct Me {
    id: String,
    name: String,
    username: String,
    avatar: String,
    rank: Option<&'static str>,
    starfish_licensed: bool,
}

pub fn router() -> Router<AppState> {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            origin.to_str().is_ok_and(session::is_urchin)
        }))
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST]);

    Router::new()
        .route("/auth/discord", get(sign_in))
        .route("/auth/discord/callback", get(callback))
        .route("/auth/me", get(me))
        .route("/auth/logout", post(sign_out))
        .layer(cors)
}

async fn sign_in(
    State(state): State<AppState>,
    Query(query): Query<SignInQuery>,
) -> Result<Response, ApiError> {
    let config = require_starfish(&state)?;
    let nonce = session::random_token();
    let return_to = session::return_address(query.return_to.as_deref());
    session::start_sign_in(&state, &nonce, &return_to).await?;
    let authorize = Url::parse_with_params(
        DISCORD_AUTHORIZE,
        [
            ("client_id", config.discord_client_id.as_str()),
            ("redirect_uri", CALLBACK),
            ("response_type", "code"),
            ("scope", "identify"),
            ("state", &nonce),
        ],
    )
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok((
        AppendHeaders([(header::SET_COOKIE, session::state_cookie(&nonce))]),
        Redirect::to(authorize.as_str()),
    )
        .into_response())
}

async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let clear_state = (header::SET_COOKIE, session::cleared_state_cookie());
    let nonce = query.state.as_deref().unwrap_or_default();
    let Some(return_to) = session::finish_sign_in(&state, &headers, nonce).await else {
        return (AppendHeaders([clear_state]), Redirect::to(session::SITE)).into_response();
    };
    match start_session(&state, query.code.as_deref()).await {
        Ok(token) => (
            AppendHeaders([
                clear_state,
                (header::SET_COOKIE, web_session::cookie(&token)),
            ]),
            Redirect::to(&return_to),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!("web sign-in failed: {error:?}");
            (AppendHeaders([clear_state]), Redirect::to(&return_to)).into_response()
        }
    }
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Me>, ApiError> {
    let user = signed_in(&state, &headers).await?;
    let member = MemberRepository::new(state.db.pool())
        .get_by_discord_id(user.discord_id)
        .await?;
    Ok(Json(Me {
        id: user.discord_id.to_string(),
        rank: member
            .map(|member| AccessRank::from_level(member.access_level))
            .filter(|rank| *rank > AccessRank::Default)
            .map(AccessRank::label),
        starfish_licensed: starfish_licensed(&state, user.discord_id).await?,
        name: user.name,
        username: user.username,
        avatar: user.avatar,
    }))
}

async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let from_urchin = headers
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .is_some_and(session::is_urchin);
    if !from_urchin {
        return Err(ApiError::Forbidden("Sign out from an Urchin site".into()));
    }
    session::end_session(&state, &headers).await?;
    Ok((
        AppendHeaders([(header::SET_COOKIE, web_session::cleared_cookie())]),
        StatusCode::NO_CONTENT,
    )
        .into_response())
}

async fn signed_in(state: &AppState, headers: &HeaderMap) -> Result<WebUser, ApiError> {
    session::current_user(state, headers)
        .await
        .ok_or_else(|| ApiError::Unauthorized("Not signed in".into()))
}

async fn starfish_licensed(state: &AppState, discord_id: i64) -> Result<bool, ApiError> {
    Ok(StarfishRepository::new(state.db.pool())
        .get_user_by_discord_id(discord_id)
        .await?
        .is_some_and(|user| user.license_status == "active"))
}

async fn start_session(state: &AppState, code: Option<&str>) -> Result<String, ApiError> {
    let config = require_starfish(state)?;
    let code = code.ok_or_else(|| ApiError::BadRequest("Discord sent no code".into()))?;
    let user = discord_user(&config, code).await?;
    let discord_id: i64 = user
        .id
        .parse()
        .map_err(|_| ApiError::Upstream("Discord sent an invalid user ID".into()))?;
    let user = WebUser {
        discord_id,
        avatar: avatar_url(discord_id, user.avatar.as_deref()),
        name: user.global_name.unwrap_or_else(|| user.username.clone()),
        username: user.username,
    };
    session::start_session(state, &user).await
}

async fn discord_user(config: &StarfishConfig, code: &str) -> Result<DiscordUser, ApiError> {
    let http = reqwest::Client::new();
    let token: DiscordToken = http
        .post(DISCORD_TOKEN)
        .form(&[
            ("client_id", config.discord_client_id.as_str()),
            ("client_secret", config.discord_client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", CALLBACK),
        ])
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| ApiError::Upstream(format!("Discord token exchange failed: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::Upstream(format!("Discord token response unreadable: {e}")))?;
    http.get(DISCORD_USER)
        .bearer_auth(token.access_token)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| ApiError::Upstream(format!("Discord user lookup failed: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::Upstream(format!("Discord user response unreadable: {e}")))
}

fn avatar_url(discord_id: i64, avatar: Option<&str>) -> String {
    match avatar {
        Some(hash) => format!("https://cdn.discordapp.com/avatars/{discord_id}/{hash}.png?size=64"),
        None => format!(
            "https://cdn.discordapp.com/embed/avatars/{}.png",
            (discord_id >> 22) % 6
        ),
    }
}
