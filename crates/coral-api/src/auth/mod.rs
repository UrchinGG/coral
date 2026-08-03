use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use coral_redis::RateLimitResult;
use database::{
    AccessRank, DeveloperKeyRepository, Member, MemberRepository, PluginRegistryRepository,
    StarfishRepository,
};

use crate::routes::starfish::auth::validate_hwid;
use crate::routes::starfish::session_auth::resolve_verified_session;
use crate::routes::starfish::{is_owner, require_starfish};
use crate::{error::ApiError, state::AppState};

const ELEVATED_CANONICAL_PATH: &str = "/v3/hypixel/player";
const ELEVATED_STRIPPED_PATH: &str = "/hypixel/player";
const ATTEST_MAX_SKEW_SECS: i64 = 120;
const PLUGIN_ATTEST_PUBKEY_HEX: &str =
    "fc250918338b1ae4b9306e2cb6a9e561d745342d2aeecc3f82b3ee36cd936548";

fn plugin_attest_pubkey() -> &'static VerifyingKey {
    static KEY: std::sync::OnceLock<VerifyingKey> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        let bytes: [u8; 32] = hex::decode(PLUGIN_ATTEST_PUBKEY_HEX)
            .expect("attestation public key must be valid hex")
            .try_into()
            .expect("attestation public key must be 32 bytes");
        VerifyingKey::from_bytes(&bytes)
            .expect("attestation public key must be a valid ed25519 key")
    })
}

#[derive(Clone)]
pub struct AuthenticatedMember(pub Member);

#[derive(Clone, Copy)]
pub struct RequestIdentity {
    pub member_id: i64,
}

impl From<&Member> for RequestIdentity {
    fn from(member: &Member) -> Self {
        Self {
            member_id: member.id,
        }
    }
}

#[derive(Clone)]
pub struct SessionAuth {
    pub discord_id: i64,
}

#[derive(Clone)]
pub struct InternalAuth;

#[derive(Clone)]
pub struct DeveloperKeyAuth {
    pub permissions: i64,
}

impl DeveloperKeyAuth {
    pub fn has(&self, perm: i64) -> bool {
        self.permissions & perm != 0
    }

    pub fn require(&self, perm: i64) -> Result<(), ApiError> {
        if self.has(perm) {
            Ok(())
        } else {
            Err(ApiError::Forbidden("insufficient permissions".into()))
        }
    }
}

enum AuthResult {
    Personal(Member),
    Developer(Member, i64),
}

pub async fn require_internal_or_developer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    if let Some(api_key) = extract_api_key(&request) {
        authorize_developer_or_internal(&state, &api_key, &mut request)
            .await
            .map_err(IntoResponse::into_response)?;
    } else {
        authorize_elevated_session(&state, &mut request)
            .await
            .map_err(IntoResponse::into_response)?;
    }
    Ok(run_with_identity(request, next).await)
}

async fn run_with_identity(request: Request, next: Next) -> Response {
    let identity = request
        .extensions()
        .get::<AuthenticatedMember>()
        .map(|m| RequestIdentity::from(&m.0));
    let mut response = next.run(request).await;
    if let Some(identity) = identity {
        response.extensions_mut().insert(identity);
    }
    response
}

async fn authorize_developer_or_internal(
    state: &AppState,
    api_key: &str,
    request: &mut Request,
) -> Result<(), StatusCode> {
    if is_internal_key(state, api_key) {
        request.extensions_mut().insert(InternalAuth);
        return Ok(());
    }

    match authenticate_key(state, api_key).await? {
        AuthResult::Personal(_) => Err(StatusCode::FORBIDDEN),
        AuthResult::Developer(member, permissions) => {
            request.extensions_mut().insert(AuthenticatedMember(member));
            request
                .extensions_mut()
                .insert(DeveloperKeyAuth { permissions });
            Ok(())
        }
    }
}

async fn authorize_elevated_session(
    state: &AppState,
    request: &mut Request,
) -> Result<(), ApiError> {
    let triad = SessionTriad::from_request(request)
        .ok_or_else(|| ApiError::Unauthorized("missing credentials".into()))?;

    let (discord_id, member) = resolve_session_member(state, &triad).await?;
    let owner = is_owner(discord_id);

    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    if !owner && !elevated_endpoint_allowed(&method, &path) {
        return Err(ApiError::Unauthorized(
            "endpoint not available for session auth".into(),
        ));
    }

    if !owner {
        let attestation = PluginAttestation::from_request(request)
            .ok_or_else(|| ApiError::Forbidden("attestation required".into()))?;

        verify_attestation(plugin_attest_pubkey(), &method, &triad.token, &attestation)?;
        verify_official_plugin(state, &attestation.slug, &attestation.plugin_hash).await?;
    }

    request.extensions_mut().insert(AuthenticatedMember(member));
    request.extensions_mut().insert(SessionAuth { discord_id });
    Ok(())
}

pub async fn require_moderator(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let api_key = extract_api_key(&request).ok_or(StatusCode::UNAUTHORIZED)?;
    let member = authenticate_personal_key(&state, &api_key).await?;
    if AccessRank::from_level(member.access_level) < AccessRank::Moderator {
        return Err(StatusCode::FORBIDDEN);
    }
    request.extensions_mut().insert(AuthenticatedMember(member));
    Ok(run_with_identity(request, next).await)
}

pub async fn allow_key_or_starfish_session(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    if let Some(api_key) = extract_api_key(&request) {
        authorize_with_key(&state, &api_key, &mut request)
            .await
            .map_err(IntoResponse::into_response)?;
    } else {
        authorize_with_session(&state, &mut request)
            .await
            .map_err(IntoResponse::into_response)?;
    }
    Ok(run_with_identity(request, next).await)
}

async fn authorize_with_key(
    state: &AppState,
    api_key: &str,
    request: &mut Request,
) -> Result<(), StatusCode> {
    if is_internal_key(state, api_key) {
        request.extensions_mut().insert(InternalAuth);
        return Ok(());
    }

    match authenticate_key(state, api_key).await? {
        AuthResult::Personal(member) => {
            request.extensions_mut().insert(AuthenticatedMember(member));
        }
        AuthResult::Developer(member, permissions) => {
            request.extensions_mut().insert(AuthenticatedMember(member));
            request
                .extensions_mut()
                .insert(DeveloperKeyAuth { permissions });
        }
    }
    Ok(())
}

async fn authorize_with_session(state: &AppState, request: &mut Request) -> Result<(), ApiError> {
    let triad = SessionTriad::from_request(request)
        .ok_or_else(|| ApiError::Unauthorized("missing credentials".into()))?;

    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    if !session_endpoint_allowed(&method, &path) {
        return Err(ApiError::Unauthorized(
            "endpoint not available for session auth".into(),
        ));
    }

    let (discord_id, member) = resolve_session_member(state, &triad).await?;
    if is_tag_write(&method, &path) {
        require_home_guild_membership(state, discord_id).await?;
    }

    request.extensions_mut().insert(AuthenticatedMember(member));
    request.extensions_mut().insert(SessionAuth { discord_id });
    Ok(())
}

async fn require_home_guild_membership(state: &AppState, discord_id: i64) -> Result<(), ApiError> {
    let guild_id = state
        .home_guild_id
        .ok_or_else(|| ApiError::ServiceUnavailable("home guild not configured".into()))?;
    match state
        .discord
        .is_guild_member(guild_id, discord_id as u64)
        .await
    {
        Some(true) => Ok(()),
        Some(false) => Err(ApiError::Forbidden(
            "must be in the Urchin server to tag".into(),
        )),
        None => Err(ApiError::ServiceUnavailable(
            "could not verify server membership".into(),
        )),
    }
}

async fn resolve_session_member(
    state: &AppState,
    triad: &SessionTriad,
) -> Result<(i64, Member), ApiError> {
    require_starfish(state)?;
    validate_hwid(&triad.hwid)?;

    let sessions = StarfishRepository::new(state.db.pool());
    let verified =
        resolve_verified_session(&sessions, &triad.token, &triad.hwid, &triad.signature).await?;
    let discord_id = verified.user.discord_id;

    session_rate_limit(state, discord_id).await?;

    let members = MemberRepository::new(state.db.pool());
    let member = match members.get_by_discord_id(discord_id).await? {
        Some(member) => member,
        None => members.create(discord_id).await?,
    };

    if member.key_locked {
        return Err(ApiError::Forbidden("account locked".into()));
    }
    Ok((discord_id, member))
}

fn elevated_endpoint_allowed(method: &Method, path: &str) -> bool {
    method == Method::GET && path == ELEVATED_STRIPPED_PATH
}

fn verify_attestation(
    pubkey: &VerifyingKey,
    method: &Method,
    session_token: &str,
    attestation: &PluginAttestation,
) -> Result<(), ApiError> {
    if (Utc::now().timestamp() - attestation.timestamp).abs() > ATTEST_MAX_SKEW_SECS {
        return Err(ApiError::Unauthorized("attestation expired".into()));
    }

    let payload = canonical_attestation_payload(
        method.as_str(),
        ELEVATED_CANONICAL_PATH,
        session_token,
        attestation.timestamp,
        &attestation.slug,
        &attestation.plugin_hash,
    );
    let signature = Signature::from_slice(&attestation.signature)
        .map_err(|_| ApiError::Unauthorized("malformed attestation".into()))?;
    pubkey
        .verify(&payload, &signature)
        .map_err(|_| ApiError::Unauthorized("attestation verification failed".into()))
}

async fn verify_official_plugin(
    state: &AppState,
    slug: &str,
    plugin_hash: &str,
) -> Result<(), ApiError> {
    let plugins = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugins
        .get_plugin_by_slug(slug)
        .await?
        .ok_or_else(|| ApiError::Forbidden("unknown plugin".into()))?;
    if !plugin.official {
        return Err(ApiError::Forbidden("plugin is not official".into()));
    }
    let release = plugins
        .get_latest_release(plugin.id)
        .await?
        .ok_or_else(|| ApiError::Forbidden("plugin has no release".into()))?;
    let content_sha256 = release
        .content_sha256
        .ok_or_else(|| ApiError::Forbidden("plugin has no content hash".into()))?;
    if hex::encode(&content_sha256) != plugin_hash {
        return Err(ApiError::Forbidden("plugin hash mismatch".into()));
    }
    Ok(())
}

fn canonical_attestation_payload(
    method: &str,
    path: &str,
    session_token: &str,
    timestamp: i64,
    slug: &str,
    plugin_hash: &str,
) -> Vec<u8> {
    format!("{method}\n{path}\n{session_token}\n{timestamp}\n{slug}\n{plugin_hash}").into_bytes()
}

struct PluginAttestation {
    slug: String,
    plugin_hash: String,
    timestamp: i64,
    signature: Vec<u8>,
}

impl PluginAttestation {
    fn from_request(request: &Request) -> Option<Self> {
        Some(Self {
            slug: session_header(request, "X-Starfish-Plugin")?,
            plugin_hash: session_header(request, "X-Starfish-Plugin-Hash")?,
            timestamp: session_header(request, "X-Starfish-Timestamp")?
                .parse()
                .ok()?,
            signature: hex::decode(session_header(request, "X-Starfish-Attest")?).ok()?,
        })
    }
}

async fn session_rate_limit(state: &AppState, discord_id: i64) -> Result<(), ApiError> {
    match state
        .rate_limiter
        .check_and_record(&format!("sf:{discord_id}"), SESSION_RATE_LIMIT)
        .await
    {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(ApiError::RateLimited),
        Err(_) => Ok(()),
    }
}

fn session_endpoint_allowed(method: &Method, path: &str) -> bool {
    matches!(
        (method.as_str(), path),
        ("GET", "/player/tags")
            | ("POST", "/players")
            | ("GET", "/player/sessions/monthly")
            | ("GET", "/player/sessions/custom")
            | ("GET", "/player/winstreaks")
    ) || is_tag_write(method, path)
}

fn is_tag_write(method: &Method, path: &str) -> bool {
    path == "/tags" && matches!(method.as_str(), "POST" | "PATCH" | "DELETE")
}

struct SessionTriad {
    token: String,
    hwid: String,
    signature: String,
}

impl SessionTriad {
    fn from_request(request: &Request) -> Option<Self> {
        Some(Self {
            token: session_header(request, "X-Starfish-Session")?,
            hwid: session_header(request, "X-Starfish-HWID")?,
            signature: session_header(request, "X-Starfish-Signature")?,
        })
    }
}

fn session_header(request: &Request, name: &str) -> Option<String> {
    request.headers().get(name)?.to_str().ok().map(String::from)
}

async fn authenticate_key(state: &AppState, api_key: &str) -> Result<AuthResult, StatusCode> {
    if let Some(member) = MemberRepository::new(state.db.pool())
        .get_by_api_key(api_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        if member.key_locked {
            return Err(StatusCode::FORBIDDEN);
        }
        check_rate_limit(state, api_key, PERSONAL_RATE_LIMIT).await?;
        if let Err(e) = MemberRepository::new(state.db.pool())
            .increment_request_count(api_key)
            .await
        {
            tracing::warn!("Failed to increment request count for member key: {e}");
        }
        return Ok(AuthResult::Personal(member));
    }

    if let Some(dev_key) = DeveloperKeyRepository::new(state.db.pool())
        .get_by_api_key(api_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        if dev_key.locked {
            return Err(StatusCode::FORBIDDEN);
        }
        let member = MemberRepository::new(state.db.pool())
            .get_by_id(dev_key.member_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        check_rate_limit(state, api_key, dev_key.rate_limit as i64).await?;
        if let Err(e) = DeveloperKeyRepository::new(state.db.pool())
            .increment_request_count(api_key)
            .await
        {
            tracing::warn!("Failed to increment request count for developer key: {e}");
        }
        return Ok(AuthResult::Developer(member, dev_key.permissions));
    }

    Err(StatusCode::UNAUTHORIZED)
}

async fn authenticate_personal_key(state: &AppState, api_key: &str) -> Result<Member, StatusCode> {
    let member = MemberRepository::new(state.db.pool())
        .get_by_api_key(api_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if member.key_locked {
        return Err(StatusCode::FORBIDDEN);
    }
    check_rate_limit(state, api_key, PERSONAL_RATE_LIMIT).await?;
    Ok(member)
}

async fn check_rate_limit(state: &AppState, api_key: &str, limit: i64) -> Result<(), StatusCode> {
    match state.rate_limiter.check_and_record(api_key, limit).await {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(StatusCode::TOO_MANY_REQUESTS),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub use coral_redis::{PERSONAL_RATE_LIMIT, SESSION_RATE_LIMIT, SESSION_UUID_BUDGET};

fn is_internal_key(state: &AppState, api_key: &str) -> bool {
    state
        .internal_api_key
        .as_ref()
        .is_some_and(|k| k == api_key)
}

fn extract_api_key(request: &Request) -> Option<String> {
    if let Some(header) = request.headers().get("X-API-Key") {
        return header.to_str().ok().map(String::from);
    }
    request.uri().query().and_then(|q| {
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == "key")
            .map(|(_, v)| v.into_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;

    #[test]
    fn session_allowlist_matches_urchin_routes() {
        assert!(session_endpoint_allowed(&Method::GET, "/player/tags"));
        assert!(session_endpoint_allowed(&Method::POST, "/players"));
        assert!(session_endpoint_allowed(
            &Method::GET,
            "/player/sessions/monthly"
        ));
        assert!(session_endpoint_allowed(
            &Method::GET,
            "/player/sessions/custom"
        ));
        assert!(session_endpoint_allowed(&Method::GET, "/player/winstreaks"));
        assert!(session_endpoint_allowed(&Method::POST, "/tags"));
        assert!(session_endpoint_allowed(&Method::PATCH, "/tags"));
        assert!(session_endpoint_allowed(&Method::DELETE, "/tags"));
    }

    #[test]
    fn session_allowlist_rejects_other_routes() {
        assert!(!session_endpoint_allowed(
            &Method::GET,
            "/player/sessions/daily"
        ));
        assert!(!session_endpoint_allowed(
            &Method::GET,
            "/player/sessions/yearly"
        ));
        assert!(!session_endpoint_allowed(&Method::GET, "/guild"));
        assert!(!session_endpoint_allowed(&Method::GET, "/hypixel/player"));
        assert!(!session_endpoint_allowed(&Method::GET, "/tags"));
        assert!(!session_endpoint_allowed(&Method::POST, "/player/lock"));
    }

    #[test]
    fn tag_write_detection() {
        assert!(is_tag_write(&Method::POST, "/tags"));
        assert!(is_tag_write(&Method::PATCH, "/tags"));
        assert!(is_tag_write(&Method::DELETE, "/tags"));
        assert!(!is_tag_write(&Method::GET, "/tags"));
        assert!(!is_tag_write(&Method::POST, "/players"));
    }

    #[test]
    fn elevated_endpoint_is_hypixel_proxy_only() {
        assert!(elevated_endpoint_allowed(&Method::GET, "/hypixel/player"));
        assert!(!elevated_endpoint_allowed(&Method::POST, "/hypixel/player"));
        assert!(!elevated_endpoint_allowed(&Method::GET, "/hypixel/guild"));
        assert!(!elevated_endpoint_allowed(&Method::GET, "/player/tags"));
    }

    fn signing_key() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
    }

    fn signed_attestation(
        session_token: &str,
        timestamp: i64,
        slug: &str,
        plugin_hash: &str,
    ) -> PluginAttestation {
        let payload = canonical_attestation_payload(
            "GET",
            ELEVATED_CANONICAL_PATH,
            session_token,
            timestamp,
            slug,
            plugin_hash,
        );
        let signature = signing_key().sign(&payload).to_bytes().to_vec();
        PluginAttestation {
            slug: slug.into(),
            plugin_hash: plugin_hash.into(),
            timestamp,
            signature,
        }
    }

    #[test]
    fn attestation_roundtrip_verifies() {
        let pubkey = signing_key().verifying_key();
        let now = Utc::now().timestamp();
        let attestation = signed_attestation("session-abc", now, "bw", "deadbeef");
        assert!(verify_attestation(&pubkey, &Method::GET, "session-abc", &attestation).is_ok());
    }

    #[test]
    fn attestation_rejects_tampered_fields() {
        let pubkey = signing_key().verifying_key();
        let now = Utc::now().timestamp();
        let mut attestation = signed_attestation("session-abc", now, "bw", "deadbeef");
        attestation.slug = "urchin".into();
        assert!(verify_attestation(&pubkey, &Method::GET, "session-abc", &attestation).is_err());

        let attestation = signed_attestation("session-abc", now, "bw", "deadbeef");
        assert!(
            verify_attestation(&pubkey, &Method::GET, "different-session", &attestation).is_err()
        );
    }

    #[test]
    fn attestation_rejects_stale_timestamp() {
        let pubkey = signing_key().verifying_key();
        let stale = Utc::now().timestamp() - ATTEST_MAX_SKEW_SECS - 5;
        let attestation = signed_attestation("session-abc", stale, "bw", "deadbeef");
        assert!(verify_attestation(&pubkey, &Method::GET, "session-abc", &attestation).is_err());
    }
}
