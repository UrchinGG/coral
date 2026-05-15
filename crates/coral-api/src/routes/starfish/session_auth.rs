use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;

use database::{StarfishRepository, starfish::StarfishUser};

use crate::{error::ApiError, state::AppState};

use super::auth::{SESSION_MAX_LIFETIME_DAYS, SESSION_SLIDING_HOURS, validate_hwid};


#[derive(Clone)]
pub struct AuthenticatedStarfishUser {
    pub user: StarfishUser,
    pub session_token: String,
    pub hwid: String,
}


pub async fn require_starfish_session(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    super::require_starfish(&state)?;

    let token = header(&request, "X-Starfish-Session")?;
    let hwid = header(&request, "X-Starfish-HWID")?;
    let signature = header(&request, "X-Starfish-Signature")?;

    validate_hwid(&hwid)?;

    let repo = StarfishRepository::new(state.db.pool());

    let session = repo.get_session_by_token(&token).await?
        .ok_or_else(|| ApiError::Unauthorized("invalid_session".into()))?;

    if Utc::now() > session.expires_at {
        return Err(ApiError::Unauthorized("invalid_session".into()));
    }

    let hwid_record = repo.get_hwid_by_id(session.hwid_id).await?
        .ok_or_else(|| ApiError::Unauthorized("invalid_session".into()))?;

    if hwid_record.hwid_hash != hwid {
        return Err(ApiError::Unauthorized("invalid_session".into()));
    }

    let expected_sig = hex::decode(&signature).ok();
    if expected_sig.as_deref() != Some(session.signature.as_slice()) {
        return Err(ApiError::Unauthorized("invalid_session".into()));
    }

    let user = repo.get_user_by_id(session.user_id).await?
        .ok_or_else(|| ApiError::Internal("session references missing user".into()))?;

    if user.license_status != "active" {
        return Err(ApiError::Forbidden("license_inactive".into()));
    }

    repo.update_heartbeat_sliding(&token, SESSION_SLIDING_HOURS, SESSION_MAX_LIFETIME_DAYS).await.ok();

    request.extensions_mut().insert(AuthenticatedStarfishUser {
        user, session_token: token, hwid,
    });

    Ok(next.run(request).await)
}


fn header(request: &Request, name: &str) -> Result<String, ApiError> {
    request.headers().get(name)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .ok_or_else(|| ApiError::Unauthorized(format!("missing {name}")))
}
