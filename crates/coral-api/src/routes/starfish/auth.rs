use axum::{Json, Router, extract::State, routing::post};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use database::{StarfishRepository, starfish::HwidComponents};
use starfish_crypto::{build_attestation_payload, ed25519_sign, base64_encode, encrypt_core_data, generate_refresh_token, generate_session_token, hash_for_attestation, hash_refresh_token, sign_unlock_key};

use crate::{error::ApiError, state::{AppState, StarfishConfig}};

use super::{rate_limit, require_starfish};
const DISCORD_DEVICE_AUTH_URL: &str = "https://discord.com/api/v10/oauth2/device/authorize";
const DISCORD_TOKEN_URL: &str = "https://discord.com/api/v10/oauth2/token";
const DISCORD_AUTHORIZE_URL: &str = "https://discord.com/oauth2/authorize";
const DISCORD_USER_URL: &str = "https://discord.com/api/v10/users/@me";
const HWID_LEN: usize = 64;
pub const SESSION_SLIDING_HOURS: i64 = 2;
pub const SESSION_MAX_LIFETIME_DAYS: i64 = 7;


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/device", post(request_device_code))
        .route("/auth/poll", post(poll_for_token))
        .route("/auth/oauth-url", post(get_oauth_url))
        .route("/auth/oauth-callback", post(oauth_callback))
        .route("/auth/refresh", post(refresh_session))
}

pub fn validate_hwid(hwid: &str) -> Result<(), ApiError> {
    if hwid.len() != HWID_LEN {
        return Err(ApiError::BadRequest("Invalid HWID format".into()));
    }
    if !hwid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest("Invalid HWID format".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct DeviceCodeRequest {
    pub hwid: String,
}

#[derive(Serialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Deserialize)]
struct DiscordDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: u64,
}

async fn request_device_code(
    State(state): State<AppState>,
    Json(req): Json<DeviceCodeRequest>,
) -> Result<Json<DeviceCodeResponse>, ApiError> {
    let config = require_starfish(&state)?;
    validate_hwid(&req.hwid)?;
    rate_limit(&state, &format!("sf:auth:{}", req.hwid), 10).await?;

    let response = reqwest::Client::new()
        .post(DISCORD_DEVICE_AUTH_URL)
        .basic_auth(&config.discord_client_id, Some(&config.discord_client_secret))
        .form(&[("scope", "identify")])
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("Discord API error: {e}")))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        tracing::error!("Discord device code error: {body}");
        return Err(ApiError::ExternalApi("Failed to get device code from Discord".into()));
    }

    let discord: DiscordDeviceCodeResponse = response.json().await
        .map_err(|e| ApiError::ExternalApi(format!("Failed to parse Discord response: {e}")))?;

    let expires_at = Utc::now() + Duration::seconds(discord.expires_in as i64);
    let repo = StarfishRepository::new(state.db.pool());
    repo.create_device_code(&discord.device_code, &discord.user_code, &req.hwid, expires_at).await?;

    let verification_uri_complete = discord.verification_uri_complete.unwrap_or_else(
        || format!("{}?user_code={}", discord.verification_uri, discord.user_code),
    );

    Ok(Json(DeviceCodeResponse {
        device_code: discord.device_code,
        user_code: discord.user_code,
        verification_uri: discord.verification_uri,
        verification_uri_complete,
        expires_in: discord.expires_in,
        interval: discord.interval,
    }))
}

#[derive(Deserialize)]
pub struct OAuthUrlRequest {
    pub redirect_port: u16,
}

#[derive(Serialize)]
pub struct OAuthUrlResponse {
    pub url: String,
    pub state: String,
}

async fn get_oauth_url(
    State(state): State<AppState>,
    Json(req): Json<OAuthUrlRequest>,
) -> Result<Json<OAuthUrlResponse>, ApiError> {
    let config = require_starfish(&state)?;

    let state_bytes: [u8; 16] = rand::random();
    let oauth_state = hex::encode(state_bytes);
    let redirect_uri = format!("http://localhost:{}/callback", req.redirect_port);

    let url = format!(
        "{DISCORD_AUTHORIZE_URL}?client_id={}&redirect_uri={}&response_type=code&scope=identify&state={oauth_state}",
        config.discord_client_id,
        urlencoding::encode(&redirect_uri),
    );

    Ok(Json(OAuthUrlResponse { url, state: oauth_state }))
}

#[derive(Deserialize)]
pub struct PollRequest {
    pub device_code: String,
    pub hwid: String,
    pub hwid_components: HwidComponents,
}

#[derive(Serialize)]
#[serde(tag = "status")]
pub enum PollResponse {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "complete")]
    Complete { unlock_key: UnlockKey },
    #[serde(rename = "error")]
    Error { error: String },
}

#[derive(Serialize, Clone)]
pub struct UnlockKey {
    pub session_token: String,
    pub core_data: String,
    pub discord_id: i64,
    pub hwid_hash: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub signature: String,
    pub server_signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct DiscordTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct DiscordErrorResponse {
    error: String,
}

#[derive(Deserialize)]
pub struct DiscordUser {
    pub id: String,
}

async fn poll_for_token(
    State(state): State<AppState>,
    Json(req): Json<PollRequest>,
) -> Result<Json<PollResponse>, ApiError> {
    let config = require_starfish(&state)?;
    rate_limit(&state, &format!("sf:poll:{}", req.hwid), 30).await?;
    let repo = StarfishRepository::new(state.db.pool());

    let stored = repo.get_device_code(&req.device_code).await?;
    let stored = match stored {
        Some(s) => s,
        None => return Ok(Json(PollResponse::Error { error: "authorization_pending".into() })),
    };

    if stored.client_hwid != req.hwid {
        return Ok(Json(PollResponse::Error { error: "authorization_pending".into() }));
    }
    if Utc::now() > stored.expires_at {
        return Ok(Json(PollResponse::Error { error: "authorization_pending".into() }));
    }

    let response = reqwest::Client::new()
        .post(DISCORD_TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", &req.device_code),
            ("client_id", &config.discord_client_id),
            ("client_secret", &config.discord_client_secret),
        ])
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("Discord API error: {e}")))?;

    let body = response.text().await.unwrap_or_default();

    if let Ok(err) = serde_json::from_str::<DiscordErrorResponse>(&body) {
        return match err.error.as_str() {
            "authorization_pending" | "slow_down" => Ok(Json(PollResponse::Pending)),
            "access_denied" => Ok(Json(PollResponse::Error { error: "access_denied".into() })),
            "expired_token" => Ok(Json(PollResponse::Error { error: "expired".into() })),
            _ => Ok(Json(PollResponse::Error { error: err.error })),
        };
    }

    let token: DiscordTokenResponse = serde_json::from_str(&body)
        .map_err(|e| ApiError::Internal(format!("Failed to parse token: {e}")))?;

    let discord_user = fetch_discord_user(&token.access_token).await?;
    let discord_id: i64 = discord_user.id.parse()
        .map_err(|_| ApiError::Internal("Invalid Discord ID".into()))?;

    let unlock_key = create_session(&config, &repo, discord_id, &req.hwid, &req.hwid_components).await?;

    repo.delete_device_code(&req.device_code).await.ok();

    Ok(Json(PollResponse::Complete { unlock_key }))
}

#[derive(Deserialize)]
pub struct OAuthCallbackRequest {
    pub code: String,
    pub redirect_port: u16,
    pub hwid: String,
    pub hwid_components: HwidComponents,
}

async fn oauth_callback(
    State(state): State<AppState>,
    Json(req): Json<OAuthCallbackRequest>,
) -> Result<Json<PollResponse>, ApiError> {
    let config = require_starfish(&state)?;
    validate_hwid(&req.hwid)?;
    rate_limit(&state, &format!("sf:auth:{}", req.hwid), 10).await?;

    let redirect_uri = format!("http://localhost:{}/callback", req.redirect_port);

    let response = reqwest::Client::new()
        .post(DISCORD_TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &req.code),
            ("redirect_uri", &redirect_uri),
            ("client_id", &config.discord_client_id),
            ("client_secret", &config.discord_client_secret),
        ])
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("Discord API error: {e}")))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        tracing::error!("Discord token exchange error: {body}");
        return Ok(Json(PollResponse::Error { error: "token_exchange_failed".into() }));
    }

    let token: DiscordTokenResponse = response.json().await
        .map_err(|e| ApiError::Internal(format!("Failed to parse token: {e}")))?;

    let discord_user = fetch_discord_user(&token.access_token).await?;
    let discord_id: i64 = discord_user.id.parse()
        .map_err(|_| ApiError::Internal("Invalid Discord ID".into()))?;

    let repo = StarfishRepository::new(state.db.pool());
    let unlock_key = create_session(&config, &repo, discord_id, &req.hwid, &req.hwid_components).await?;

    Ok(Json(PollResponse::Complete { unlock_key }))
}

pub async fn fetch_discord_user(access_token: &str) -> Result<DiscordUser, ApiError> {
    let response = reqwest::Client::new()
        .get(DISCORD_USER_URL)
        .bearer_auth(access_token)
        .send().await
        .map_err(|e| ApiError::ExternalApi(format!("Discord API error: {e}")))?;

    if !response.status().is_success() {
        return Err(ApiError::Unauthorized("Invalid Discord token".into()));
    }

    response.json().await
        .map_err(|e| ApiError::ExternalApi(format!("Failed to parse Discord response: {e}")))
}

pub async fn create_session(
    config: &StarfishConfig,
    repo: &StarfishRepository<'_>,
    discord_id: i64,
    hwid: &str,
    hwid_components: &HwidComponents,
) -> Result<UnlockKey, ApiError> {
    let user = repo.upsert_user(discord_id).await?;

    if user.license_status != "active" {
        return Err(ApiError::Forbidden("license_inactive".into()));
    }

    let hwid_record = handle_hwid_registration(repo, user.id, hwid, hwid_components).await?;

    repo.delete_user_sessions(user.id).await?;
    repo.delete_user_refresh_tokens(user.id).await?;

    let session_token = generate_session_token();
    let core_data = encrypt_core_data(&config.core_tables_bytes, &session_token, hwid);
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::hours(SESSION_SLIDING_HOURS);

    let signature = sign_unlock_key(
        discord_id, hwid,
        issued_at.timestamp() as u64,
        expires_at.timestamp() as u64,
        &config.hmac_secret,
    );

    repo.create_session(
        user.id, hwid_record.id, &session_token,
        &core_data, expires_at, &signature,
    ).await?;

    let refresh_token = generate_refresh_token();
    repo.create_refresh_token(user.id, hwid_record.id, &hash_refresh_token(&refresh_token)).await?;

    let core_data_encoded = base64_encode(&core_data);
    let core_data_hash = hash_for_attestation(&core_data);
    let issued_at_ts = issued_at.timestamp() as u64;
    let expires_at_ts = expires_at.timestamp() as u64;
    let attestation = build_attestation_payload(&session_token, discord_id, hwid, issued_at_ts, expires_at_ts, &core_data_hash);
    let server_sig = ed25519_sign(&attestation, &config.signing_key);

    Ok(UnlockKey {
        session_token,
        core_data: core_data_encoded,
        discord_id,
        hwid_hash: hwid.to_string(),
        issued_at: issued_at_ts,
        expires_at: expires_at_ts,
        signature: hex::encode(signature),
        server_signature: hex::encode(server_sig),
        refresh_token: Some(refresh_token),
    })
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
    pub hwid: String,
    pub hwid_components: HwidComponents,
}

async fn refresh_session(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<PollResponse>, ApiError> {
    let config = require_starfish(&state)?;
    validate_hwid(&req.hwid)?;
    rate_limit(&state, &format!("sf:refresh:{}", req.hwid), 5).await?;
    let repo = StarfishRepository::new(state.db.pool());

    let token_hash = hash_refresh_token(&req.refresh_token);
    let stored = repo.get_refresh_token_by_hash(&token_hash).await?
        .ok_or_else(|| ApiError::Unauthorized("invalid_refresh_token".into()))?;

    let hwid_record = repo.get_hwid_by_id(stored.hwid_id).await?
        .ok_or_else(|| ApiError::Internal("Refresh token references missing HWID".into()))?;

    if hwid_record.hwid_hash != req.hwid {
        let fuzzy_ok = repo.get_hwid_components(hwid_record.id).await?
            .is_some_and(|stored_c| req.hwid_components.match_count(&stored_c) >= 3);
        if !fuzzy_ok {
            return Err(ApiError::Unauthorized("hwid_mismatch".into()));
        }
    }

    let user = repo.get_user_by_id(stored.user_id).await?
        .ok_or_else(|| ApiError::Internal("Refresh token references missing user".into()))?;

    if user.license_status != "active" {
        repo.delete_user_refresh_tokens(user.id).await?;
        return Err(ApiError::Forbidden("license_inactive".into()));
    }

    repo.delete_user_sessions(user.id).await?;

    let session_token = generate_session_token();
    let core_data = encrypt_core_data(&config.core_tables_bytes, &session_token, &req.hwid);
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::hours(SESSION_SLIDING_HOURS);

    let signature = sign_unlock_key(
        user.discord_id, &req.hwid,
        issued_at.timestamp() as u64,
        expires_at.timestamp() as u64,
        &config.hmac_secret,
    );

    repo.create_session(
        user.id, hwid_record.id, &session_token,
        &core_data, expires_at, &signature,
    ).await?;

    let new_refresh = generate_refresh_token();
    repo.rotate_refresh_token(&token_hash, &hash_refresh_token(&new_refresh)).await?;

    let core_data_encoded = base64_encode(&core_data);
    let core_data_hash = hash_for_attestation(&core_data);
    let issued_at_ts = issued_at.timestamp() as u64;
    let expires_at_ts = expires_at.timestamp() as u64;
    let attestation = build_attestation_payload(&session_token, user.discord_id, &req.hwid, issued_at_ts, expires_at_ts, &core_data_hash);
    let server_sig = ed25519_sign(&attestation, &config.signing_key);

    Ok(Json(PollResponse::Complete {
        unlock_key: UnlockKey {
            session_token,
            core_data: core_data_encoded,
            discord_id: user.discord_id,
            hwid_hash: req.hwid,
            issued_at: issued_at_ts,
            expires_at: expires_at_ts,
            signature: hex::encode(signature),
            server_signature: hex::encode(server_sig),
            refresh_token: Some(new_refresh),
        },
    }))
}
async fn handle_hwid_registration(
    repo: &StarfishRepository<'_>,
    user_id: i64,
    hwid: &str,
    components: &HwidComponents,
) -> Result<database::starfish::StarfishHwid, ApiError> {
    if let Some(existing) = repo.get_hwid(user_id, hwid).await? {
        repo.activate_hwid(user_id, existing.id).await?;
        repo.store_hwid_components(existing.id, components).await?;
        return Ok(existing);
    }

    if let Some(fuzzy_match) = repo.find_fuzzy_hwid(user_id, components, 3).await? {
        repo.activate_hwid(user_id, fuzzy_match.id).await?;
        repo.store_hwid_components(fuzzy_match.id, components).await?;
        return Ok(fuzzy_match);
    }

    if repo.hwid_changes_since(user_id, 30).await? >= 2 {
        return Err(ApiError::BadRequest("HWID change limit reached (2 per month)".into()));
    }

    let old = repo.get_active_hwid(user_id).await?;
    let old_hash = old.as_ref().map(|h| h.hwid_hash.as_str());
    repo.record_hwid_change(user_id, old_hash, hwid).await?;

    let new_hwid = repo.register_hwid(user_id, hwid).await?;
    repo.store_hwid_components(new_hwid.id, components).await?;
    Ok(new_hwid)
}
