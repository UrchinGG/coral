use std::collections::HashSet;

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use clients::normalize_uuid;
use coral_redis::SyncEvent;
use database::{AccountRepository, CacheRepository, LinkRole, MemberRepository, permissions};
use redis::AsyncCommands;

use crate::auth::{AuthenticatedMember, DeveloperKeyAuth, InternalAuth};
use crate::cache::spawn_snapshot_store;
use crate::error::{ApiError, ErrorResponse};
use crate::routes::player::{resolve_identifier, resolve_player_data};
use crate::state::AppState;

const VERIFY_HOST: &str = "link.urchin.gg";
const FAILED_CODES_PER_USER: i64 = 10;
/// Backstop for a leaked key rotating Discord IDs, sized for a bot serving a busy server.
/// With 4-digit codes that live 120 s, a key at this ceiling hits a given live code
/// about 0.4% of the time.
const FAILED_CODES_PER_KEY: i64 = 100;
const FAILED_CODE_WINDOW_SECS: i64 = 300;
const MAX_SUGGESTIONS: usize = 25;

#[derive(Serialize, ToSchema)]
pub struct LinkedAccount {
    pub uuid: String,
    pub username: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct LinksResponse {
    pub discord_id: String,
    pub primary: Option<LinkedAccount>,
    pub alts: Vec<LinkedAccount>,
}

#[derive(Serialize, ToSchema)]
pub struct LinkMethodsResponse {
    /// Unlinked accounts whose Hypixel social settings show this member's Discord username.
    pub suggested: Vec<LinkedAccount>,
    /// The Minecraft server that hands out codes for the `code` method.
    pub verify_host: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum LinkProof {
    /// A code the player received by joining the verification server.
    Code { code: String },
    /// A username or UUID whose Hypixel social settings show the member's Discord username.
    Social { player: String },
    /// No proof. Staff only: requires the internal key.
    Force { player: String },
}

#[derive(Deserialize, ToSchema)]
pub struct LinkRequest {
    #[serde(flatten)]
    pub proof: LinkProof,
    /// Link as the primary account. Otherwise the account becomes an alt, or the
    /// primary if the member has none.
    #[serde(default)]
    pub primary: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkedAs {
    Primary,
    Alt,
}

#[derive(Serialize, ToSchema)]
pub struct LinkResponse {
    pub linked: LinkedAccount,
    pub role: LinkedAs,
    pub links: LinksResponse,
}

#[derive(Deserialize, ToSchema)]
pub struct SetPrimaryRequest {
    pub uuid: String,
}

/// Who is calling: Coral's own services, or a developer key with `LINK_ACCOUNTS`.
/// Either way the caller vouches only for which Discord user asked.
pub enum LinkCaller {
    Internal,
    Developer { member_id: i64 },
}

impl<S: Send + Sync> FromRequestParts<S> for LinkCaller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, ApiError> {
        if parts.extensions.get::<InternalAuth>().is_some() {
            return Ok(Self::Internal);
        }
        match (
            parts.extensions.get::<DeveloperKeyAuth>(),
            parts.extensions.get::<AuthenticatedMember>(),
        ) {
            (Some(key), Some(AuthenticatedMember(member)))
                if key.has(permissions::LINK_ACCOUNTS) =>
            {
                Ok(Self::Developer {
                    member_id: member.id,
                })
            }
            _ => Err(ApiError::Forbidden(
                "requires the Link Accounts permission".into(),
            )),
        }
    }
}

impl LinkCaller {
    /// Failed codes are counted per Discord user, so one player can't lock out a whole
    /// server sharing a bot's key, and per developer key as a ceiling.
    fn failed_code_buckets(&self, discord_id: u64) -> Vec<(String, i64)> {
        match self {
            Self::Internal => vec![(
                format!("link_code_failures:discord:{discord_id}"),
                FAILED_CODES_PER_USER,
            )],
            Self::Developer { member_id } => vec![
                (
                    format!("link_code_failures:key:{member_id}:discord:{discord_id}"),
                    FAILED_CODES_PER_USER,
                ),
                (
                    format!("link_code_failures:key:{member_id}"),
                    FAILED_CODES_PER_KEY,
                ),
            ],
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/links/{discord_id}", get(get_links).post(link_account))
        .route("/links/{discord_id}/methods", get(link_methods))
        .route("/links/{discord_id}/primary", put(set_primary))
        .route(
            "/links/{discord_id}/accounts/{uuid}",
            delete(unlink_account),
        )
}

#[utoipa::path(
    get,
    path = "/v3/links/{discord_id}",
    description = "Returns the Minecraft accounts linked to a Discord user. Requires the `Link Accounts` permission.",
    params(("discord_id" = String, Path, description = "Discord user ID")),
    responses(
        (status = 200, description = "Linked accounts; empty for an unknown user", body = LinksResponse),
        (status = 403, description = "Missing permission", body = ErrorResponse),
    ),
    tag = "Links",
    security(("api_key" = []))
)]
pub async fn get_links(
    State(state): State<AppState>,
    _: LinkCaller,
    Path(discord_id): Path<u64>,
) -> Result<Json<LinksResponse>, ApiError> {
    Ok(Json(links_of(&state, discord_id).await?))
}

#[utoipa::path(
    get,
    path = "/v3/links/{discord_id}/methods",
    description = "Returns what to offer a Discord user who wants to link an account: suggested accounts, which can be linked with the `social` method, and the server that hands out codes for the `code` method. Requires the `Link Accounts` permission.",
    params(("discord_id" = String, Path, description = "Discord user ID")),
    responses(
        (status = 200, description = "Link methods", body = LinkMethodsResponse),
        (status = 403, description = "Missing permission", body = ErrorResponse),
        (status = 404, description = "Unknown Discord user", body = ErrorResponse),
    ),
    tag = "Links",
    security(("api_key" = []))
)]
pub async fn link_methods(
    State(state): State<AppState>,
    _: LinkCaller,
    Path(discord_id): Path<u64>,
) -> Result<Json<LinkMethodsResponse>, ApiError> {
    let discord_name = state
        .discord
        .resolve_username(discord_id)
        .await
        .ok_or_else(|| ApiError::NotFound("Discord user not found".into()))?;
    let links = links_of(&state, discord_id).await?;
    let linked: HashSet<String> = links
        .primary
        .into_iter()
        .chain(links.alts)
        .map(|account| account.uuid)
        .collect();
    let suggested = CacheRepository::new(state.db.pool())
        .find_by_discord_username(&discord_name)
        .await?
        .into_iter()
        .filter(|(uuid, _)| !linked.contains(uuid))
        .take(MAX_SUGGESTIONS)
        .map(|(uuid, username)| LinkedAccount {
            uuid,
            username: Some(username),
        })
        .collect();
    Ok(Json(LinkMethodsResponse {
        suggested,
        verify_host: VERIFY_HOST.into(),
    }))
}

#[utoipa::path(
    post,
    path = "/v3/links/{discord_id}",
    description = "Links a Minecraft account to a Discord user once Coral has checked the proof. Coral looks up the Discord username itself; the caller only says which Discord user asked. An account linked elsewhere moves to this user. Requires the `Link Accounts` permission; the `force` method requires the internal key. Failed codes are limited to 10 per five minutes per Discord user, and 100 per developer key.",
    params(("discord_id" = String, Path, description = "Discord user ID")),
    request_body = LinkRequest,
    responses(
        (status = 200, description = "Account linked", body = LinkResponse),
        (status = 403, description = "Missing permission, or the proof does not match", body = ErrorResponse),
        (status = 404, description = "Unknown or expired code, or unknown player", body = ErrorResponse),
        (status = 409, description = "Already linked to this user", body = ErrorResponse),
        (status = 429, description = "Too many failed codes", body = ErrorResponse),
        (status = 503, description = "Hypixel is unreachable and the stored profile does not show this Discord user", body = ErrorResponse),
    ),
    tag = "Links",
    security(("api_key" = []))
)]
pub async fn link_account(
    State(state): State<AppState>,
    caller: LinkCaller,
    Path(discord_id): Path<u64>,
    Json(request): Json<LinkRequest>,
) -> Result<Json<LinkResponse>, ApiError> {
    let account = match request.proof {
        LinkProof::Code { code } => redeem_code(&state, &caller, discord_id, &code).await?,
        LinkProof::Social { player } => check_social(&state, discord_id, &player).await?,
        LinkProof::Force { player } => {
            if !matches!(caller, LinkCaller::Internal) {
                return Err(ApiError::Forbidden(
                    "force requires the internal key".into(),
                ));
            }
            find_player(&state, &player).await?
        }
    };
    let role = claim(&state, discord_id, &account, request.primary).await?;
    Ok(Json(LinkResponse {
        linked: account,
        role,
        links: links_of(&state, discord_id).await?,
    }))
}

#[utoipa::path(
    put,
    path = "/v3/links/{discord_id}/primary",
    description = "Makes one of a Discord user's alts their primary account; the old primary becomes an alt. Requires the `Link Accounts` permission.",
    params(("discord_id" = String, Path, description = "Discord user ID")),
    request_body = SetPrimaryRequest,
    responses(
        (status = 200, description = "Primary changed", body = LinksResponse),
        (status = 403, description = "Missing permission", body = ErrorResponse),
        (status = 404, description = "Not one of this user's accounts", body = ErrorResponse),
    ),
    tag = "Links",
    security(("api_key" = []))
)]
pub async fn set_primary(
    State(state): State<AppState>,
    _: LinkCaller,
    Path(discord_id): Path<u64>,
    Json(request): Json<SetPrimaryRequest>,
) -> Result<Json<LinksResponse>, ApiError> {
    let uuid = normalize_uuid(&request.uuid);
    let member = MemberRepository::new(state.db.pool())
        .get_by_discord_id(discord_id as i64)
        .await?
        .ok_or_else(not_linked)?;
    if member.uuid.as_deref() != Some(uuid.as_str()) {
        if !AccountRepository::new(state.db.pool())
            .swap_primary(member.id, &uuid)
            .await?
        {
            return Err(not_linked());
        }
        resync(&state, [discord_id]).await;
    }
    Ok(Json(links_of(&state, discord_id).await?))
}

#[utoipa::path(
    delete,
    path = "/v3/links/{discord_id}/accounts/{uuid}",
    description = "Unlinks one of a Discord user's accounts. Removing the primary leaves the user without one; no alt takes its place. Requires the `Link Accounts` permission.",
    params(
        ("discord_id" = String, Path, description = "Discord user ID"),
        ("uuid" = String, Path, description = "Minecraft UUID"),
    ),
    responses(
        (status = 200, description = "Account unlinked", body = LinksResponse),
        (status = 403, description = "Missing permission", body = ErrorResponse),
        (status = 404, description = "Not one of this user's accounts", body = ErrorResponse),
    ),
    tag = "Links",
    security(("api_key" = []))
)]
pub async fn unlink_account(
    State(state): State<AppState>,
    _: LinkCaller,
    Path((discord_id, uuid)): Path<(u64, String)>,
) -> Result<Json<LinksResponse>, ApiError> {
    let member = MemberRepository::new(state.db.pool())
        .get_by_discord_id(discord_id as i64)
        .await?
        .ok_or_else(not_linked)?;
    if !AccountRepository::new(state.db.pool())
        .unlink(member.id, &normalize_uuid(&uuid))
        .await?
    {
        return Err(not_linked());
    }
    resync(&state, [discord_id]).await;
    Ok(Json(links_of(&state, discord_id).await?))
}

async fn links_of(state: &AppState, discord_id: u64) -> Result<LinksResponse, ApiError> {
    let mut links = LinksResponse {
        discord_id: discord_id.to_string(),
        primary: None,
        alts: Vec::new(),
    };
    let Some(member) = MemberRepository::new(state.db.pool())
        .get_by_discord_id(discord_id as i64)
        .await?
    else {
        return Ok(links);
    };
    if let Some(uuid) = member.uuid {
        links.primary = Some(named(state, uuid).await);
    }
    for alt in AccountRepository::new(state.db.pool())
        .list(member.id)
        .await?
    {
        links.alts.push(named(state, alt.uuid).await);
    }
    Ok(links)
}

async fn named(state: &AppState, uuid: String) -> LinkedAccount {
    let username = CacheRepository::new(state.db.pool())
        .get_username(&uuid)
        .await
        .ok()
        .flatten();
    LinkedAccount { uuid, username }
}

async fn redeem_code(
    state: &AppState,
    caller: &LinkCaller,
    discord_id: u64,
    code: &str,
) -> Result<LinkedAccount, ApiError> {
    let mut redis = state.redis.connection();
    let buckets = caller.failed_code_buckets(discord_id);
    for (bucket, limit) in &buckets {
        let failures: Option<i64> = redis.get(bucket).await.map_err(redis_failure)?;
        if failures.unwrap_or(0) >= *limit {
            return Err(ApiError::RateLimited);
        }
    }
    let Some(player) = coral_redis::verify::redeem_code(&mut redis, code).await else {
        for (bucket, _) in &buckets {
            let failures: i64 = redis.incr(bucket, 1).await.map_err(redis_failure)?;
            if failures == 1 {
                let _: () = redis
                    .expire(bucket, FAILED_CODE_WINDOW_SECS)
                    .await
                    .map_err(redis_failure)?;
            }
        }
        return Err(ApiError::NotFound("invalid or expired code".into()));
    };
    Ok(LinkedAccount {
        uuid: player.uuid.simple().to_string(),
        username: Some(player.username),
    })
}

async fn check_social(
    state: &AppState,
    discord_id: u64,
    player: &str,
) -> Result<LinkedAccount, ApiError> {
    let discord_name = state
        .discord
        .current_username(discord_id)
        .await
        .ok_or_else(|| ApiError::NotFound("Discord user not found".into()))?;
    let (uuid, name_hint) = resolve_identifier(state, player).await?;
    let (data, stale) = resolve_player_data(state, &uuid, Some(Duration::zero())).await?;
    let data = data.ok_or_else(|| ApiError::NotFound("player has no Hypixel profile".into()))?;
    if !stale {
        spawn_snapshot_store(state, &uuid, &data, name_hint.clone());
    }
    if !shows_discord(&data, &discord_name) {
        return Err(if stale {
            ApiError::ServiceUnavailable(
                "Hypixel is unreachable and the stored profile does not show this Discord user"
                    .into(),
            )
        } else {
            ApiError::Forbidden("Hypixel social settings do not show this Discord user".into())
        });
    }
    let username = name_hint.or_else(|| data["displayname"].as_str().map(String::from));
    Ok(LinkedAccount { uuid, username })
}

async fn find_player(state: &AppState, player: &str) -> Result<LinkedAccount, ApiError> {
    let (uuid, name_hint) = resolve_identifier(state, player).await?;
    match name_hint {
        Some(username) => Ok(LinkedAccount {
            uuid,
            username: Some(username),
        }),
        None => Ok(named(state, uuid).await),
    }
}

fn shows_discord(player: &Value, discord_name: &str) -> bool {
    player["socialMedia"]["links"]["DISCORD"]
        .as_str()
        .is_some_and(|linked| linked.eq_ignore_ascii_case(discord_name))
}

async fn claim(
    state: &AppState,
    discord_id: u64,
    account: &LinkedAccount,
    primary: bool,
) -> Result<LinkedAs, ApiError> {
    let member = MemberRepository::new(state.db.pool())
        .create(discord_id as i64)
        .await?;
    let accounts = AccountRepository::new(state.db.pool());
    let already_linked = if primary {
        member.uuid.as_deref() == Some(account.uuid.as_str())
    } else {
        accounts
            .is_owned_by(&account.uuid, discord_id as i64)
            .await?
    };
    if already_linked {
        return Err(ApiError::Conflict("account already linked".into()));
    }
    let role = if primary || member.uuid.is_none() {
        LinkRole::Primary
    } else {
        LinkRole::Alt
    };
    let displaced = accounts.claim(member.id, &account.uuid, role).await?;
    if let Some(username) = &account.username {
        let _ = CacheRepository::new(state.db.pool())
            .cache_username(&account.uuid, username)
            .await;
    }
    resync(
        state,
        std::iter::once(discord_id).chain(displaced.into_iter().map(|id| id as u64)),
    )
    .await;
    Ok(match role {
        LinkRole::Primary => LinkedAs::Primary,
        LinkRole::Alt => LinkedAs::Alt,
    })
}

async fn resync(state: &AppState, discord_ids: impl IntoIterator<Item = u64>) {
    for discord_id in discord_ids {
        state
            .sync_events
            .publish(&SyncEvent::SyncUser { discord_id })
            .await;
    }
}

fn redis_failure(error: redis::RedisError) -> ApiError {
    ApiError::Internal(format!("link code limit: {error}"))
}

fn not_linked() -> ApiError {
    ApiError::NotFound("account not linked to this user".into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn social_match_ignores_case() {
        let player = json!({"socialMedia": {"links": {"DISCORD": "Nelly"}}});
        assert!(shows_discord(&player, "nelly"));
        assert!(!shows_discord(&player, "nelly2"));
        assert!(!shows_discord(&json!({}), "nelly"));
    }

    #[test]
    fn proofs_parse_by_method() {
        let code: LinkRequest =
            serde_json::from_value(json!({"method": "code", "code": "1234"})).unwrap();
        assert!(matches!(code.proof, LinkProof::Code { ref code } if code == "1234"));
        assert!(!code.primary);

        let social: LinkRequest =
            serde_json::from_value(json!({"method": "social", "player": "Notch", "primary": true}))
                .unwrap();
        assert!(matches!(social.proof, LinkProof::Social { .. }));
        assert!(social.primary);

        assert!(serde_json::from_value::<LinkRequest>(json!({"method": "trust_me"})).is_err());
    }
}

#[cfg(test)]
mod api_tests {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use clients::MojangClient;
    use coral_redis::RedisPool;
    use database::Database;
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    const INTERNAL_KEY: &str = "links-internal-key";

    struct Api {
        router: Router,
        state: AppState,
        _log: reqlog::LogReceiver,
    }

    async fn api() -> Option<Api> {
        dotenvy::dotenv().ok();
        let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?;
        let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
            .await
            .ok()?;
        let state = AppState::new(
            db,
            None,
            MojangClient::new(redis.connection()),
            None,
            Some(INTERNAL_KEY.into()),
            redis,
            None,
            None,
            None,
        );
        let (request_log, log) = reqlog::channel();
        Some(Api {
            router: crate::build_router(state.clone(), request_log),
            state,
            _log: log,
        })
    }

    impl Api {
        async fn call(&self, method: Method, uri: &str, key: &str, body: Value) -> (u16, Value) {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("X-API-Key", key)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            let response = self.router.clone().oneshot(request).await.unwrap();
            let status = response.status().as_u16();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            (
                status,
                serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            )
        }

        async fn developer(&self, permissions: i64) -> String {
            let discord_id = discord_id() as i64;
            let member = MemberRepository::new(self.state.db.pool())
                .create(discord_id)
                .await
                .unwrap();
            let api_key = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO developer_keys (member_id, api_key, permissions) VALUES ($1, $2, $3)",
            )
            .bind(member.id)
            .bind(&api_key)
            .bind(permissions)
            .execute(self.state.db.pool())
            .await
            .unwrap();
            api_key
        }

        async fn code_for(&self, uuid: &str) -> String {
            loop {
                let code = format!("{:04}", Uuid::new_v4().as_u128() % 10_000);
                let stored = coral_redis::verify::store_code(
                    &mut self.state.redis.connection(),
                    &code,
                    Uuid::parse_str(uuid).unwrap(),
                    "Linked",
                )
                .await
                .unwrap();
                if stored {
                    return code;
                }
            }
        }

        async fn link_by_code(&self, key: &str, discord_id: u64, uuid: &str) -> (u16, Value) {
            let code = self.code_for(uuid).await;
            self.call(
                Method::POST,
                &format!("/v3/links/{discord_id}"),
                key,
                json!({"method": "code", "code": code}),
            )
            .await
        }
    }

    fn discord_id() -> u64 {
        880_000_000_000_000_000 + u64::from(Uuid::new_v4().as_fields().1)
    }

    fn player() -> String {
        Uuid::new_v4().simple().to_string()
    }

    #[tokio::test]
    async fn the_link_accounts_permission_is_required() {
        let Some(api) = api().await else {
            return;
        };
        let without = api.developer(permissions::PLAYER_DATA).await;
        let with = api.developer(permissions::LINK_ACCOUNTS).await;
        let uri = format!("/v3/links/{}", discord_id());

        assert_eq!(
            api.call(Method::GET, &uri, &without, Value::Null).await.0,
            403
        );
        let (status, links) = api.call(Method::GET, &uri, &with, Value::Null).await;
        assert_eq!(status, 200);
        assert_eq!(links["primary"], Value::Null);
        assert_eq!(links["alts"], json!([]));
    }

    #[tokio::test]
    async fn codes_link_a_primary_then_alts_and_the_primary_can_move() {
        let Some(api) = api().await else {
            return;
        };
        let key = api.developer(permissions::LINK_ACCOUNTS).await;
        let member = discord_id();
        let (first, second) = (player(), player());

        let (status, body) = api.link_by_code(&key, member, &first).await;
        assert_eq!(status, 200);
        assert_eq!(body["role"], "primary");
        assert_eq!(body["linked"]["username"], "Linked");

        let (status, body) = api.link_by_code(&key, member, &second).await;
        assert_eq!(status, 200);
        assert_eq!(body["role"], "alt");
        assert_eq!(body["links"]["alts"][0]["uuid"], second);

        assert_eq!(api.link_by_code(&key, member, &second).await.0, 409);

        let primary = format!("/v3/links/{member}/primary");
        let (status, links) = api
            .call(Method::PUT, &primary, &key, json!({"uuid": second}))
            .await;
        assert_eq!(status, 200);
        assert_eq!(links["primary"]["uuid"], second);
        assert_eq!(links["alts"][0]["uuid"], first);
        assert_eq!(
            api.call(Method::PUT, &primary, &key, json!({"uuid": player()}))
                .await
                .0,
            404
        );

        let remove = format!("/v3/links/{member}/accounts/{second}");
        let (status, links) = api.call(Method::DELETE, &remove, &key, Value::Null).await;
        assert_eq!(status, 200);
        assert_eq!(links["primary"], Value::Null);
        assert_eq!(
            api.call(Method::DELETE, &remove, &key, Value::Null).await.0,
            404
        );
    }

    #[tokio::test]
    async fn a_linked_account_moves_to_its_new_owner() {
        let Some(api) = api().await else {
            return;
        };
        let key = api.developer(permissions::LINK_ACCOUNTS).await;
        let (old_owner, new_owner, account) = (discord_id(), discord_id(), player());

        api.link_by_code(&key, old_owner, &account).await;
        assert_eq!(api.link_by_code(&key, new_owner, &account).await.0, 200);

        let (_, old_links) = api
            .call(
                Method::GET,
                &format!("/v3/links/{old_owner}"),
                &key,
                Value::Null,
            )
            .await;
        assert_eq!(old_links["primary"], Value::Null);
    }

    #[tokio::test]
    async fn force_needs_the_internal_key() {
        let Some(api) = api().await else {
            return;
        };
        let key = api.developer(permissions::LINK_ACCOUNTS).await;
        let uri = format!("/v3/links/{}", discord_id());
        let force = json!({"method": "force", "player": player(), "primary": true});

        assert_eq!(
            api.call(Method::POST, &uri, &key, force.clone()).await.0,
            403
        );
        let (status, body) = api.call(Method::POST, &uri, INTERNAL_KEY, force).await;
        assert_eq!(status, 200);
        assert_eq!(body["role"], "primary");
    }

    #[tokio::test]
    async fn failed_codes_are_limited_per_user_then_per_key() {
        let Some(api) = api().await else {
            return;
        };
        let key = api.developer(permissions::LINK_ACCOUNTS).await;
        let guess = json!({"method": "code", "code": "not-a-code"});
        let guesser = format!("/v3/links/{}", discord_id());
        for _ in 0..FAILED_CODES_PER_USER {
            assert_eq!(
                api.call(Method::POST, &guesser, &key, guess.clone())
                    .await
                    .0,
                404
            );
        }
        assert_eq!(
            api.call(Method::POST, &guesser, &key, guess.clone())
                .await
                .0,
            429
        );

        let (status, _) = api.link_by_code(&key, discord_id(), &player()).await;
        assert_eq!(status, 200, "another player on the same key still links");

        for _ in FAILED_CODES_PER_USER..FAILED_CODES_PER_KEY {
            let uri = format!("/v3/links/{}", discord_id());
            assert_eq!(
                api.call(Method::POST, &uri, &key, guess.clone()).await.0,
                404
            );
        }
        let uri = format!("/v3/links/{}", discord_id());
        assert_eq!(api.call(Method::POST, &uri, &key, guess).await.0, 429);
    }

    #[tokio::test]
    async fn correct_codes_do_not_count_as_failures() {
        let Some(api) = api().await else {
            return;
        };
        let key = api.developer(permissions::LINK_ACCOUNTS).await;
        for _ in 0..=FAILED_CODES_PER_USER {
            let (status, _) = api.link_by_code(&key, discord_id(), &player()).await;
            assert_eq!(status, 200);
        }
    }

    #[tokio::test]
    async fn only_the_internal_key_redeems_verify_codes() {
        let Some(api) = api().await else {
            return;
        };
        let key = api
            .developer(permissions::LINK_ACCOUNTS | permissions::HYPIXEL)
            .await;
        let code = api.code_for(&player()).await;
        let uri = format!("/v3/verify/codes/{code}");

        assert_eq!(
            api.call(Method::DELETE, &uri, &key, Value::Null).await.0,
            403
        );
        assert_eq!(
            api.call(Method::DELETE, &uri, INTERNAL_KEY, Value::Null)
                .await
                .0,
            200
        );
    }
}
