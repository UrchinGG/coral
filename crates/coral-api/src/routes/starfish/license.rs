use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use database::starfish::HwidComponents;

use crate::{error::ApiError, state::AppState};

use super::{
    auth::{SESSION_MAX_LIFETIME_DAYS, SESSION_SLIDING_HOURS, UnlockKey, build_unlock_key},
    rate_limit, require_starfish, resolve_discord_user,
    session_auth::resolve_verified_session,
};

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
    Json(ValidateResponse {
        valid: false,
        unlock_key: None,
        reason: Some(reason.into()),
    })
}

async fn validate_session(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, ApiError> {
    let config = require_starfish(&state)?;
    super::auth::validate_hwid(&req.hwid)?;
    rate_limit(&state, &format!("sf:sess:{}", req.session_token), 30).await?;
    let repo = database::StarfishRepository::new(state.db.pool());

    let verified = match resolve_verified_session(
        &repo,
        &req.session_token,
        &req.hwid,
        &req.signature,
    )
    .await
    {
        Ok(v) => v,
        Err(_) => return Ok(failed_validation("invalid_session")),
    };

    repo.update_heartbeat_sliding(
        &req.session_token,
        SESSION_SLIDING_HOURS,
        SESSION_MAX_LIFETIME_DAYS,
    )
    .await
    .ok();

    let unlock_key = build_unlock_key(
        &config,
        &req.session_token,
        verified.user.discord_id,
        &req.hwid,
        verified.session.issued_at.timestamp() as u64,
        verified.session.expires_at.timestamp() as u64,
        &verified.session.signature,
        None,
    );

    Ok(Json(ValidateResponse {
        valid: true,
        unlock_key: Some(unlock_key),
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
    Json(HeartbeatResponse {
        status: "invalid".into(),
        next_heartbeat_in: None,
        reason: Some(reason.into()),
    })
}

async fn heartbeat(
    State(state): State<AppState>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ApiError> {
    require_starfish(&state)?;
    super::auth::validate_hwid(&req.hwid)?;
    rate_limit(&state, &format!("sf:sess:{}", req.session_token), 30).await?;
    let repo = database::StarfishRepository::new(state.db.pool());

    if let Err(e) =
        resolve_verified_session(&repo, &req.session_token, &req.hwid, &req.signature).await
    {
        return Ok(fail_heartbeat(match e {
            ApiError::Forbidden(_) => "license_revoked",
            _ => "invalid",
        }));
    }

    repo.update_heartbeat_sliding(
        &req.session_token,
        SESSION_SLIDING_HOURS,
        SESSION_MAX_LIFETIME_DAYS,
    )
    .await?;

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

    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::Unauthorized("Invalid credentials".into()))?;

    let has_license = resolve_discord_user(&state, token)
        .await?
        .is_some_and(|u| u.license_status == "active");

    Ok(Json(LicenseStatusResponse { has_license }))
}
