use axum::{Json, Router, extract::State, http::HeaderMap, routing::{get, post}};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use database::{StarfishRepository, starfish::HwidComponents};
use starfish_crypto::{build_attestation_payload, ed25519_sign, base64_encode, encrypt_core_data, hash_for_attestation};

use crate::{error::ApiError, state::AppState};

use super::{auth::{UnlockKey, fetch_discord_user, SESSION_SLIDING_HOURS, SESSION_MAX_LIFETIME_DAYS}, rate_limit, require_starfish};


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/license/validate", post(validate_session))
        .route("/heartbeat", post(heartbeat))
        .route("/license/check", get(check_license))
}

#[derive(Deserialize)]
pub struct ValidateRequest {
    pub session_token: String,
    pub hwid: String,
    pub signature: String,
    pub hwid_components: HwidComponents,
}

#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlock_key: Option<UnlockKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn failed_validation(reason: &str) -> Json<ValidateResponse> {
    Json(ValidateResponse { valid: false, unlock_key: None, reason: Some(reason.into()) })
}

async fn validate_session(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, ApiError> {
    let config = require_starfish(&state)?;
    super::auth::validate_hwid(&req.hwid)?;
    rate_limit(&state, &format!("sf:sess:{}", req.session_token), 30).await?;
    let repo = StarfishRepository::new(state.db.pool());

    let session = match repo.get_session_by_token(&req.session_token).await? {
        Some(s) => s,
        None => return Ok(failed_validation("invalid_session")),
    };

    if Utc::now() > session.expires_at {
        return Ok(failed_validation("invalid_session"));
    }

    match repo.get_hwid_by_id(session.hwid_id).await? {
        Some(h) if h.hwid_hash == req.hwid => {}
        _ => return Ok(failed_validation("invalid_session")),
    }

    let user = repo.get_user_by_id(session.user_id).await?
        .ok_or_else(|| ApiError::Internal("Session references missing user".into()))?;

    if user.license_status != "active" {
        return Ok(failed_validation("invalid_session"));
    }

    let provided = hex::decode(&req.signature)
        .ok()
        .filter(|s| s.as_slice() == session.signature.as_slice());

    if provided.is_none() {
        return Ok(failed_validation("invalid_session"));
    }

    repo.update_heartbeat_sliding(&req.session_token, SESSION_SLIDING_HOURS, SESSION_MAX_LIFETIME_DAYS).await.ok();

    let core_data = encrypt_core_data(&config.core_tables_bytes, &req.session_token, &req.hwid);
    let core_data_encoded = base64_encode(&core_data);
    let core_data_hash = hash_for_attestation(&core_data);
    let issued_at_ts = session.issued_at.timestamp() as u64;
    let expires_at_ts = session.expires_at.timestamp() as u64;
    let attestation = build_attestation_payload(&req.session_token, user.discord_id, &req.hwid, issued_at_ts, expires_at_ts, &core_data_hash);
    let server_sig = ed25519_sign(&attestation, &config.signing_key);

    Ok(Json(ValidateResponse {
        valid: true,
        unlock_key: Some(UnlockKey {
            session_token: req.session_token.clone(),
            core_data: core_data_encoded,
            discord_id: user.discord_id,
            hwid_hash: req.hwid,
            issued_at: issued_at_ts,
            expires_at: expires_at_ts,
            signature: hex::encode(&session.signature),
            server_signature: hex::encode(server_sig),
            refresh_token: None,
        }),
        reason: None,
    }))
}

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub session_token: String,
    pub hwid: String,
    pub signature: String,
    pub hwid_components: HwidComponents,
}

#[derive(Serialize)]
pub struct HeartbeatResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_heartbeat_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn fail_heartbeat(reason: &str) -> Json<HeartbeatResponse> {
    Json(HeartbeatResponse { status: "invalid".into(), next_heartbeat_in: None, reason: Some(reason.into()) })
}

async fn heartbeat(
    State(state): State<AppState>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ApiError> {
    require_starfish(&state)?;
    super::auth::validate_hwid(&req.hwid)?;
    rate_limit(&state, &format!("sf:sess:{}", req.session_token), 30).await?;
    let repo = StarfishRepository::new(state.db.pool());

    let session = match repo.get_session_by_token(&req.session_token).await? {
        Some(s) => s,
        None => return Ok(fail_heartbeat("invalid")),
    };

    if Utc::now() > session.expires_at {
        return Ok(fail_heartbeat("invalid"));
    }

    match repo.get_hwid_by_id(session.hwid_id).await? {
        Some(h) if h.hwid_hash == req.hwid => {}
        _ => return Ok(fail_heartbeat("invalid")),
    }

    let provided = hex::decode(&req.signature)
        .ok()
        .filter(|s| s.as_slice() == session.signature.as_slice());

    if provided.is_none() {
        return Ok(fail_heartbeat("invalid"));
    }

    let user = repo.get_user_by_id(session.user_id).await?
        .ok_or_else(|| ApiError::Internal("Session references missing user".into()))?;

    if user.license_status != "active" {
        return Ok(fail_heartbeat("license_revoked"));
    }

    repo.update_heartbeat_sliding(&req.session_token, SESSION_SLIDING_HOURS, SESSION_MAX_LIFETIME_DAYS).await?;

    Ok(Json(HeartbeatResponse {
        status: "ok".into(),
        next_heartbeat_in: Some(60),
        reason: None,
    }))
}

#[derive(Serialize)]
pub struct LicenseStatusResponse {
    pub has_license: bool,
}

async fn check_license(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<LicenseStatusResponse>, ApiError> {
    require_starfish(&state)?;

    let token = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::Unauthorized("Invalid credentials".into()))?;

    let discord_user = fetch_discord_user(token).await?;
    let discord_id: i64 = discord_user.id.parse()
        .map_err(|_| ApiError::Internal("Invalid Discord ID".into()))?;

    let repo = StarfishRepository::new(state.db.pool());
    let has_license = repo.get_user_by_discord_id(discord_id).await?
        .is_some_and(|u| u.license_status == "active");

    Ok(Json(LicenseStatusResponse { has_license }))
}
