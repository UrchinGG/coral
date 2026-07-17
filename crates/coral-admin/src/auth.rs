use axum::extract::{Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};

use crate::session::{self, SESSION_COOKIE_NAME};
use crate::state::AppState;

#[derive(Clone, Copy)]
pub struct AdminActor {
    pub discord_id: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", get(logout))
        .route("/auth/me", get(me))
}

pub async fn require_owner(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let discord_id = session_discord_id(&request, &state).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.owner_ids.contains(&discord_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    request.extensions_mut().insert(AdminActor { discord_id });
    Ok(next.run(request).await)
}

fn session_discord_id(request: &Request, state: &AppState) -> Option<i64> {
    let cookies = request.headers().get(header::COOKIE)?.to_str().ok()?;
    let token = cookies
        .split(';')
        .map(str::trim)
        .find_map(|kv| kv.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")))?;
    session::verify(token, &state.session_secret)
}

async fn login(State(state): State<AppState>) -> Redirect {
    let redirect_uri = callback_url(&state);
    let url = format!(
        "https://discord.com/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope=identify",
        state.oauth.client_id,
        urlencoding::encode(&redirect_uri),
    );
    Redirect::temporary(&url)
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
}

async fn callback(State(state): State<AppState>, Query(params): Query<CallbackParams>) -> Response {
    let Some(code) = params.code else {
        return Redirect::temporary("/?error=no_code").into_response();
    };

    match exchange_code(&state, &code).await {
        Some(user) => match user.id.parse::<i64>() {
            Ok(discord_id) if state.owner_ids.contains(&discord_id) => {
                let token = session::issue(discord_id, &state.session_secret);
                let cookie = format!(
                    "{SESSION_COOKIE_NAME}={token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
                    session::SESSION_TTL.num_seconds(),
                );
                ([(header::SET_COOKIE, cookie)], Redirect::temporary("/")).into_response()
            }
            _ => Redirect::temporary("/?error=not_authorized").into_response(),
        },
        None => Redirect::temporary("/?error=auth_failed").into_response(),
    }
}

async fn exchange_code(state: &AppState, code: &str) -> Option<DiscordUser> {
    let redirect_uri = callback_url(state);
    let token_res = state
        .http
        .post("https://discord.com/api/v10/oauth2/token")
        .form(&[
            ("client_id", state.oauth.client_id.as_str()),
            ("client_secret", state.oauth.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri),
        ])
        .send()
        .await
        .ok()?;
    if !token_res.status().is_success() {
        return None;
    }
    let token: TokenResponse = token_res.json().await.ok()?;

    let user_res = state
        .http
        .get("https://discord.com/api/v10/users/@me")
        .bearer_auth(&token.access_token)
        .send()
        .await
        .ok()?;
    if !user_res.status().is_success() {
        return None;
    }
    user_res.json().await.ok()
}

fn callback_url(state: &AppState) -> String {
    format!("{}/auth/callback", state.oauth.base_url)
}

async fn logout() -> impl IntoResponse {
    let cookie =
        format!("{SESSION_COOKIE_NAME}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0");
    ([(header::SET_COOKIE, cookie)], Redirect::temporary("/"))
}

#[derive(Serialize)]
struct MeResponse {
    authenticated: bool,
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    discord_id: Option<i64>,
}

async fn me(State(state): State<AppState>, request: Request) -> Json<MeResponse> {
    let discord_id = session_discord_id(&request, &state).filter(|id| state.owner_ids.contains(id));
    Json(MeResponse {
        authenticated: discord_id.is_some(),
        discord_id,
    })
}
