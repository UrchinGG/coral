use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{OriginalUri, Request, State};
use axum::http::uri::PathAndQuery;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use database::{AccessRank, Database, MemberRepository};
use url::Url;

use crate::perimeter::MOUNT;
use crate::security::json_error;
use crate::session::{self, SESSION_COOKIE, SessionStore};

const ASSETS_PREFIX: &str = "/assets/";
const SIGN_IN_URL: &str = "https://urchin.gg/auth/discord";

#[derive(Clone, Copy)]
pub struct AdminActor {
    pub discord_id: i64,
    pub rank: AccessRank,
}

pub struct AuthConfig {
    pub owner_ids: HashSet<i64>,
    pub base_url: Url,
    pub sessions: Arc<dyn SessionStore>,
    pub ranks: Arc<dyn StaffRanks>,
}

/// Where a signed-in Discord account's Urchin access rank is read from.
#[async_trait]
pub trait StaffRanks: Send + Sync {
    async fn rank(&self, discord_id: i64) -> AccessRank;
}

#[async_trait]
impl StaffRanks for Database {
    async fn rank(&self, discord_id: i64) -> AccessRank {
        match MemberRepository::new(self.pool())
            .get_by_discord_id(discord_id)
            .await
        {
            Ok(member) => member.map_or(AccessRank::Default, |member| {
                AccessRank::from_level(member.access_level)
            }),
            Err(error) => {
                tracing::error!("could not read the access rank of {discord_id}: {error}");
                AccessRank::Default
            }
        }
    }
}

impl AuthConfig {
    async fn discord_id(&self, headers: &HeaderMap) -> Option<i64> {
        let token = session::cookie_value(headers, SESSION_COOKIE)?;
        self.sessions.discord_id(token).await
    }

    async fn signed_in(&self, headers: &HeaderMap) -> Option<AdminActor> {
        let discord_id = self.discord_id(headers).await?;
        let rank = self.rank(discord_id).await;
        Some(AdminActor { discord_id, rank })
    }

    pub async fn rank(&self, discord_id: i64) -> AccessRank {
        if self.owner_ids.contains(&discord_id) {
            return AccessRank::Owner;
        }
        self.ranks.rank(discord_id).await
    }

    fn sign_in_url(&self, requested: &str) -> String {
        let return_to = format!(
            "{}{requested}",
            self.base_url.origin().ascii_serialization()
        );
        Url::parse_with_params(SIGN_IN_URL, [("return", return_to.as_str())])
            .expect("the sign-in URL is a valid absolute URL")
            .to_string()
    }
}

pub async fn require_signed_in(
    State(auth): State<Arc<AuthConfig>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(actor) = auth.signed_in(request.headers()).await else {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Sign in to continue",
        );
    };
    request.extensions_mut().insert(actor);
    next.run(request).await
}

pub async fn require_signed_in_page(
    State(auth): State<Arc<AuthConfig>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(actor) = auth.signed_in(request.headers()).await {
        request.extensions_mut().insert(actor);
        return next.run(request).await;
    }
    let is_page_navigation = matches!(*request.method(), Method::GET | Method::HEAD)
        && !request.uri().path().starts_with(ASSETS_PREFIX);
    if !is_page_navigation {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let requested = request
        .extensions()
        .get::<OriginalUri>()
        .and_then(|original| original.0.path_and_query().map(PathAndQuery::as_str))
        .unwrap_or(MOUNT);
    Redirect::to(&auth.sign_in_url(requested)).into_response()
}
