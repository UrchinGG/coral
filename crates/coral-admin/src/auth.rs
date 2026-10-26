use std::collections::HashSet;
use std::sync::Arc;

use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, Request, State};
use axum::http::uri::PathAndQuery;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{AppendHeaders, Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use coral_redis::web_session;
use serde::Deserialize;
use url::Url;

use crate::security::{self, ROOT_PATH, json_error};
use crate::session::{self, SESSION_COOKIE, SessionStore};

const LOGIN_PATH: &str = "/login";
const LOGOUT_PATH: &str = "/auth/logout";
const ASSETS_PREFIX: &str = "/assets/";
const SIGN_IN_URL: &str = "https://api.urchin.gg/auth/discord";
const LOGIN_TEMPLATE: &str = include_str!("login.html");
const SIGN_IN_PROMPT: &str = r#"<p>Sign in with the Discord account registered as an owner to continue.</p>
<a href="{sign_in}">Sign in with Discord</a>"#;
const NOT_AN_OWNER: &str = r#"<p class="notice">That Discord account is not an owner.</p>
<form method="post" action="/auth/logout"><button>Sign out</button></form>"#;

#[derive(Clone, Copy)]
pub struct AdminActor {
    pub discord_id: i64,
}

pub struct AuthConfig {
    pub owner_ids: HashSet<i64>,
    pub base_url: Url,
    pub sessions: Arc<dyn SessionStore>,
}

#[derive(Deserialize)]
struct LoginQuery {
    #[serde(default)]
    next: String,
}

impl AuthConfig {
    async fn discord_id(&self, headers: &HeaderMap) -> Option<i64> {
        let token = session::cookie_value(headers, SESSION_COOKIE)?;
        self.sessions.discord_id(token).await
    }

    async fn owner(&self, headers: &HeaderMap) -> Option<AdminActor> {
        let discord_id = self.discord_id(headers).await?;
        self.owner_ids
            .contains(&discord_id)
            .then_some(AdminActor { discord_id })
    }

    fn sign_in_url(&self, next: &str) -> String {
        let return_to = self
            .base_url
            .join(next)
            .unwrap_or_else(|_| self.base_url.clone());
        Url::parse_with_params(SIGN_IN_URL, [("return", return_to.as_str())])
            .expect("the sign-in URL is a valid absolute URL")
            .to_string()
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
        .route(LOGOUT_PATH, logout_route)
        .with_state(auth)
}

pub async fn require_owner(
    State(auth): State<Arc<AuthConfig>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(actor) = auth.owner(request.headers()).await else {
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
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(actor) = auth.owner(request.headers()).await {
        request.extensions_mut().insert(actor);
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
    let requested = query.map_or_else(|_| String::new(), |Query(query)| query.next);
    let next = security::safe_next(&requested, &auth.base_url);
    let body = match auth.discord_id(&headers).await {
        Some(id) if auth.owner_ids.contains(&id) => return Redirect::to(&next).into_response(),
        Some(_) => NOT_AN_OWNER.to_string(),
        None => SIGN_IN_PROMPT.replace("{sign_in}", &auth.sign_in_url(&next)),
    };
    Html(LOGIN_TEMPLATE.replace("{body}", &body)).into_response()
}

async fn logout(State(auth): State<Arc<AuthConfig>>, headers: HeaderMap) -> Response {
    if let Some(token) = session::cookie_value(&headers, SESSION_COOKIE)
        && let Err(error) = auth.sessions.end(token).await
    {
        tracing::error!("admin logout could not end the session: {error:#}");
    }
    (
        AppendHeaders([(header::SET_COOKIE, web_session::cleared_cookie())]),
        Redirect::to(LOGIN_PATH),
    )
        .into_response()
}

fn login_redirect(uri: &Uri) -> Response {
    let requested = uri.path_and_query().map_or(ROOT_PATH, PathAndQuery::as_str);
    let login = format!("{LOGIN_PATH}?next={}", urlencoding::encode(requested));
    Redirect::to(&login).into_response()
}
