use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, Request, State};
use axum::http::uri::PathAndQuery;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{AppendHeaders, Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use url::Url;

use crate::epoch::SessionEpoch;
use crate::security::{self, ROOT_PATH, json_error};
use crate::session::{
    self, AuthKeys, OAUTH_STATE_COOKIE, OAUTH_STATE_TTL, OAuthState, SESSION_COOKIE, SESSION_TTL,
    Session,
};

const LOGIN_PATH: &str = "/login";
const START_LOGIN_PATH: &str = "/auth/login";
const CALLBACK_PATH: &str = "/auth/callback";
const LOGOUT_PATH: &str = "/auth/logout";
const ASSETS_PREFIX: &str = "/assets/";
const DISCORD_AUTHORIZE_URL: &str = "https://discord.com/oauth2/authorize";
const DISCORD_API_URL: &str = "https://discord.com/api/v10/";
const DISCORD_TOKEN_PATH: &str = "oauth2/token";
const DISCORD_USER_PATH: &str = "users/@me";
const DISCORD_SCOPE: &str = "identify";
const DISCORD_TIMEOUT: Duration = Duration::from_secs(5);
const LOGIN_TEMPLATE: &str = include_str!("login.html");

#[derive(Clone, Copy)]
pub struct AdminActor {
    pub discord_id: i64,
}

pub struct AuthConfig {
    pub owner_ids: HashSet<i64>,
    pub base_url: Url,
    pub keys: AuthKeys,
    pub epoch: Arc<SessionEpoch>,
    pub discord: DiscordOAuth,
}

pub struct DiscordOAuth {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    token_url: Url,
    user_url: Url,
    http: reqwest::Client,
}

struct GrantedLogin {
    session_cookie: String,
    next: String,
}

#[derive(Clone, Copy)]
enum LoginNotice {
    Denied,
    Failed,
    NotRevoked,
}

#[derive(Debug, thiserror::Error)]
enum LoginError {
    #[error("the callback query is malformed: {0}")]
    MalformedCallback(QueryRejection),
    #[error("the OAuth state cookie is missing, expired or does not match")]
    InvalidState,
    #[error("Discord returned no authorization code")]
    MissingCode,
    #[error("Discord identification failed: {0:#}")]
    Discord(anyhow::Error),
    #[error("Discord account {0} is not an owner")]
    NotOwner(i64),
}

#[derive(Default, Deserialize)]
struct LoginQuery {
    #[serde(default)]
    next: String,
    error: Option<String>,
}

#[derive(Default, Deserialize)]
struct StartQuery {
    #[serde(default)]
    next: String,
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
}

impl AuthConfig {
    fn owner(&self, headers: &HeaderMap) -> Option<AdminActor> {
        let token = session::cookie_value(headers, SESSION_COOKIE)?;
        let session = Session::open(token, &self.keys.session)?;
        let is_current_owner =
            session.epoch == self.epoch.current() && self.owner_ids.contains(&session.discord_id);
        is_current_owner.then_some(AdminActor {
            discord_id: session.discord_id,
        })
    }
}

impl DiscordOAuth {
    pub fn new(client_id: String, client_secret: String, base_url: &Url) -> Result<Self> {
        let api_url = Url::parse(DISCORD_API_URL).context("parse the Discord API URL")?;
        Self::with_api(client_id, client_secret, base_url, &api_url)
    }

    pub fn with_api(
        client_id: String,
        client_secret: String,
        base_url: &Url,
        api_url: &Url,
    ) -> Result<Self> {
        let redirect_uri = base_url
            .join(CALLBACK_PATH)
            .context("build the OAuth redirect URI from ADMIN_BASE_URL")?
            .to_string();
        let http = reqwest::Client::builder()
            .timeout(DISCORD_TIMEOUT)
            .build()
            .context("build the Discord OAuth HTTP client")?;
        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            token_url: api_url
                .join(DISCORD_TOKEN_PATH)
                .context("build the Discord token URL")?,
            user_url: api_url
                .join(DISCORD_USER_PATH)
                .context("build the Discord user URL")?,
            http,
        })
    }

    fn authorize_url(&self, state: &OAuthState) -> String {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", DISCORD_SCOPE),
            ("state", state.nonce.as_str()),
        ];
        Url::parse_with_params(DISCORD_AUTHORIZE_URL, params)
            .expect("the Discord authorize URL is a valid absolute URL")
            .to_string()
    }

    async fn identify(&self, code: &str) -> Result<i64> {
        let token: TokenResponse = self
            .http
            .post(self.token_url.as_str())
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
            ])
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .context("exchange the authorization code")?
            .json()
            .await
            .context("decode the token response")?;
        let user: DiscordUser = self
            .http
            .get(self.user_url.as_str())
            .bearer_auth(&token.access_token)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .context("fetch the Discord user")?
            .json()
            .await
            .context("decode the Discord user")?;
        user.id.parse().context("parse the Discord user id")
    }
}

impl LoginNotice {
    const ALL: [Self; 3] = [Self::Denied, Self::Failed, Self::NotRevoked];

    fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|notice| notice.code() == code)
    }

    fn code(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Failed => "failed",
            Self::NotRevoked => "not_revoked",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Denied => "That Discord account is not an owner.",
            Self::Failed => "Sign-in failed. Try again.",
            Self::NotRevoked => {
                "Signed out here, but other sessions could not be revoked. Sign in and log out again."
            }
        }
    }
}

impl LoginError {
    fn notice(&self) -> LoginNotice {
        match self {
            Self::NotOwner(_) => LoginNotice::Denied,
            Self::MalformedCallback(_)
            | Self::InvalidState
            | Self::MissingCode
            | Self::Discord(_) => LoginNotice::Failed,
        }
    }
}

pub fn router<S>(auth: Arc<AuthConfig>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let logout_route = post(logout).layer(from_fn_with_state(
        auth.clone(),
        security::same_origin_writes,
    ));
    Router::new()
        .route(LOGIN_PATH, get(login_page))
        .route(START_LOGIN_PATH, get(start_login))
        .route(CALLBACK_PATH, get(finish_login))
        .route(LOGOUT_PATH, logout_route)
        .with_state(auth)
}

pub async fn require_owner(
    State(auth): State<Arc<AuthConfig>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(actor) = auth.owner(request.headers()) else {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Sign in as an owner",
        );
    };
    request.extensions_mut().insert(actor);
    next.run(request).await
}

pub async fn require_owner_page(
    State(auth): State<Arc<AuthConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if auth.owner(request.headers()).is_some() {
        return next.run(request).await;
    }
    let is_page_navigation = matches!(*request.method(), Method::GET | Method::HEAD)
        && !request.uri().path().starts_with(ASSETS_PREFIX);
    if !is_page_navigation {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    login_redirect(request.uri())
}

async fn login_page(
    State(auth): State<Arc<AuthConfig>>,
    headers: HeaderMap,
    query: Result<Query<LoginQuery>, QueryRejection>,
) -> Response {
    let query = query_or_empty(query);
    let next = security::safe_next(&query.next, &auth.base_url);
    if auth.owner(&headers).is_some() {
        return Redirect::to(&next).into_response();
    }
    let notice = query.error.as_deref().and_then(LoginNotice::from_code);
    Html(render_login_page(&next, notice)).into_response()
}

async fn start_login(
    State(auth): State<Arc<AuthConfig>>,
    query: Result<Query<StartQuery>, QueryRejection>,
) -> Response {
    let query = query_or_empty(query);
    let state = OAuthState::start(security::safe_next(&query.next, &auth.base_url));
    let cookie = session::set_cookie(
        OAUTH_STATE_COOKIE,
        &state.seal(&auth.keys.oauth_state),
        OAUTH_STATE_TTL,
    );
    (
        AppendHeaders([(header::SET_COOKIE, cookie)]),
        Redirect::to(&auth.discord.authorize_url(&state)),
    )
        .into_response()
}

async fn finish_login(
    State(auth): State<Arc<AuthConfig>>,
    headers: HeaderMap,
    query: Result<Query<CallbackQuery>, QueryRejection>,
) -> Response {
    let clear_state = (
        header::SET_COOKIE,
        session::clear_cookie(OAUTH_STATE_COOKIE),
    );
    match complete_login(&auth, &headers, query).await {
        Ok(granted) => (
            AppendHeaders([clear_state, (header::SET_COOKIE, granted.session_cookie)]),
            Redirect::to(&granted.next),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!("admin login rejected: {error}");
            let login = login_notice_path(error.notice());
            (AppendHeaders([clear_state]), Redirect::to(&login)).into_response()
        }
    }
}

async fn complete_login(
    auth: &AuthConfig,
    headers: &HeaderMap,
    query: Result<Query<CallbackQuery>, QueryRejection>,
) -> Result<GrantedLogin, LoginError> {
    let Query(query) = query.map_err(LoginError::MalformedCallback)?;
    let state = session::cookie_value(headers, OAUTH_STATE_COOKIE)
        .and_then(|token| OAuthState::open(token, &auth.keys.oauth_state))
        .ok_or(LoginError::InvalidState)?;
    let returned_state = query.state.as_deref().ok_or(LoginError::InvalidState)?;
    if !state.matches(returned_state) {
        return Err(LoginError::InvalidState);
    }
    let code = query.code.as_deref().ok_or(LoginError::MissingCode)?;
    let discord_id = auth
        .discord
        .identify(code)
        .await
        .map_err(LoginError::Discord)?;
    if !auth.owner_ids.contains(&discord_id) {
        return Err(LoginError::NotOwner(discord_id));
    }
    let session = Session::start(discord_id, auth.epoch.current());
    Ok(GrantedLogin {
        session_cookie: session::set_cookie(
            SESSION_COOKIE,
            &session.seal(&auth.keys.session),
            SESSION_TTL,
        ),
        next: state.next,
    })
}

async fn logout(State(auth): State<Arc<AuthConfig>>, headers: HeaderMap) -> Response {
    let landing = match auth.owner(&headers) {
        Some(_) => landing_after_revocation(&auth).await,
        None => LOGIN_PATH.to_string(),
    };
    (
        AppendHeaders([(header::SET_COOKIE, session::clear_cookie(SESSION_COOKIE))]),
        Redirect::to(&landing),
    )
        .into_response()
}

async fn landing_after_revocation(auth: &AuthConfig) -> String {
    let Err(error) = auth.epoch.revoke_all().await else {
        return LOGIN_PATH.to_string();
    };
    tracing::error!("admin logout could not revoke sessions: {error:#}");
    login_notice_path(LoginNotice::NotRevoked)
}

fn query_or_empty<T: Default>(query: Result<Query<T>, QueryRejection>) -> T {
    query.map_or_else(|_| T::default(), |Query(query)| query)
}

fn login_notice_path(notice: LoginNotice) -> String {
    format!("{LOGIN_PATH}?error={}", notice.code())
}

fn login_redirect(uri: &Uri) -> Response {
    let requested = uri.path_and_query().map_or(ROOT_PATH, PathAndQuery::as_str);
    let login = format!("{LOGIN_PATH}?next={}", urlencoding::encode(requested));
    Redirect::to(&login).into_response()
}

fn render_login_page(next: &str, notice: Option<LoginNotice>) -> String {
    let sign_in = format!("{START_LOGIN_PATH}?next={}", urlencoding::encode(next));
    let notice = match notice {
        Some(notice) => format!("<p class=\"notice\">{}</p>", notice.message()),
        None => String::new(),
    };
    LOGIN_TEMPLATE
        .replace("{sign_in}", &sign_in)
        .replace("{notice}", &notice)
}
