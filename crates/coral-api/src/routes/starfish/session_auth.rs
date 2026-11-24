use std::net::IpAddr;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;
use coral_redis::RateLimitResult;
use subtle::ConstantTimeEq;

use database::{
    StarfishRepository,
    starfish::{StarfishSession, StarfishUser},
};
use reqlog::{Caller, record_caller};

use crate::{error::ApiError, reqlog::client_ip, state::AppState};

use super::auth::{SESSION_MAX_LIFETIME_DAYS, SESSION_SLIDING_HOURS, validate_hwid};

/// Addresses one session may be used from within a rate-limit window: room for a network
/// switch or an IPv4/IPv6 flip, not for handing the session to friends.
const SESSION_IP_LIMIT: i64 = 3;

#[derive(Clone)]
pub struct AuthenticatedStarfishUser {
    pub user: StarfishUser,
    pub session_token: String,
    pub hwid: String,
}

pub struct VerifiedSession {
    pub session: StarfishSession,
    pub user: StarfishUser,
}

pub async fn resolve_verified_session(
    state: &AppState,
    ip: Option<IpAddr>,
    token: &str,
    hwid: &str,
    signature_hex: &str,
) -> Result<VerifiedSession, ApiError> {
    let repo = StarfishRepository::new(state.db.pool());
    let session = repo
        .get_session_by_token(token)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("invalid_session".into()))?;

    let user = repo
        .get_user_by_id(session.user_id)
        .await?
        .ok_or_else(|| ApiError::Internal("session references missing user".into()))?;
    record_caller(Caller::session(None, user.discord_id));

    if Utc::now() > session.expires_at {
        return Err(ApiError::Unauthorized("invalid_session".into()));
    }

    let hwid_record = repo
        .get_hwid_by_id(session.hwid_id)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("invalid_session".into()))?;

    if hwid_record.hwid_hash != hwid {
        return Err(ApiError::Unauthorized("invalid_session".into()));
    }

    let presented = hex::decode(signature_hex).unwrap_or_default();
    if !bool::from(presented.ct_eq(&session.signature)) {
        return Err(ApiError::Unauthorized("invalid_session".into()));
    }

    admit_session_ip(state, token, ip).await?;

    if user.license_status != "active" {
        return Err(ApiError::Forbidden("license_inactive".into()));
    }

    Ok(VerifiedSession { session, user })
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

    let ip = client_ip(request.headers());
    let verified = resolve_verified_session(&state, ip, &token, &hwid, &signature).await?;

    StarfishRepository::new(state.db.pool())
        .update_heartbeat_sliding(&token, SESSION_SLIDING_HOURS, SESSION_MAX_LIFETIME_DAYS)
        .await
        .ok();

    request.extensions_mut().insert(AuthenticatedStarfishUser {
        user: verified.user,
        session_token: token,
        hwid,
    });

    Ok(next.run(request).await)
}

async fn admit_session_ip(
    state: &AppState,
    token: &str,
    ip: Option<IpAddr>,
) -> Result<(), ApiError> {
    let Some(ip) = ip else {
        return Ok(());
    };
    match state
        .rate_limiter
        .admit_distinct(
            &format!("sf:ips:{token}"),
            &[ip.to_string()],
            SESSION_IP_LIMIT,
        )
        .await
    {
        Ok(RateLimitResult::Exceeded { .. }) => {
            Err(ApiError::Unauthorized("session_shared".into()))
        }
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Err(e) => {
            tracing::warn!("session address check skipped, Redis failed: {e}");
            Ok(())
        }
    }
}

fn header(request: &Request, name: &str) -> Result<String, ApiError> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .ok_or_else(|| ApiError::Unauthorized(format!("missing {name}")))
}
