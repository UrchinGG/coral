use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, post};
use axum::{Extension, Json, Router};
use chrono::{TimeDelta, Utc};
use coral_redis::{BlacklistEvent, EventPublisher};
use database::{
    AccessRank, DeveloperKeyRepository, MemberRepository, StarfishRepository, StrikeRepository,
    permissions,
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::AccountTarget;
use super::profile::{DevKey, developer_key};
use super::rail::{Limiter, known_limiters};
use crate::audit;
use crate::auth::AdminActor;
use crate::error::{AdminError, Context};
use crate::state::AppState;

const STRIKE_REASON_MAX_CHARS: usize = 100;
const STRIKE_DURATION_DAYS: [i64; 2] = [14, 30];
const DEV_KEY_LABEL_MAX_CHARS: usize = 64;
const DEV_KEY_MAX_RATE_LIMIT: i32 = 100_000;
const LICENSE_STATUSES: [&str; 3] = ["inactive", "active", "suspended"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessLevelBody {
    level: i16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaggingBody {
    disabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrikeBody {
    reason: String,
    duration_days: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevKeyBody {
    label: String,
    permissions: i64,
    rate_limit: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedBody {
    locked: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitBody {
    rate_limit: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionsBody {
    permissions: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseBody {
    status: String,
}

#[derive(Serialize)]
pub struct NewApiKey {
    api_key: String,
}

#[derive(Serialize)]
pub struct ResetLimits {
    cleared: Vec<Limiter>,
}

struct Audited<'a> {
    state: &'a AppState,
    actor: AdminActor,
    target: AccountTarget,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{discord_id}/lock", post(lock))
        .route("/{discord_id}/unlock", post(unlock))
        .route("/{discord_id}/access-level", post(set_access_level))
        .route("/{discord_id}/tagging", post(set_tagging))
        .route("/{discord_id}/strikes", post(add_strike))
        .route("/{discord_id}/strikes/{strike_id}", delete(revoke_strike))
        .route("/{discord_id}/api-key/regenerate", post(regenerate_api_key))
        .route(
            "/{discord_id}/dev-key",
            post(create_dev_key).delete(delete_dev_key),
        )
        .route("/{discord_id}/dev-key/lock", post(set_dev_key_locked))
        .route(
            "/{discord_id}/dev-key/rate-limit",
            post(set_dev_key_rate_limit),
        )
        .route(
            "/{discord_id}/dev-key/permissions",
            post(set_dev_key_permissions),
        )
        .route("/{discord_id}/limits/reset", post(reset_limits))
        .route("/{discord_id}/starfish/license", post(set_license))
        .route(
            "/{discord_id}/starfish/sessions/revoke",
            post(revoke_sessions),
        )
}

async fn lock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
) -> Result<StatusCode, AdminError> {
    let audited = Audited::member(&state, actor, &raw).await?;
    MemberRepository::new(state.db.pool())
        .lock_key(audited.target.discord_id)
        .await
        .context("lock the account's key")?;
    audited.record("lock_member", json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn unlock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
) -> Result<StatusCode, AdminError> {
    let audited = Audited::member(&state, actor, &raw).await?;
    MemberRepository::new(state.db.pool())
        .unlock_key(audited.target.discord_id)
        .await
        .context("unlock the account's key")?;
    audited.record("unlock_member", json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_access_level(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<AccessLevelBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let level = access_level(body.level)?;
    let audited = Audited::member(&state, actor, &raw).await?;
    MemberRepository::new(state.db.pool())
        .set_access_level(audited.target.discord_id, level)
        .await
        .context("set the account's access level")?;
    audited
        .record("set_access_level", json!({ "level": level }))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_tagging(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<TaggingBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let audited = Audited::member(&state, actor, &raw).await?;
    MemberRepository::new(state.db.pool())
        .set_tagging_disabled(audited.target.discord_id, body.disabled)
        .await
        .context("change whether the account may tag")?;
    audited
        .record("set_tagging_disabled", json!({ "disabled": body.disabled }))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_strike(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<StrikeBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let reason = strike_reason(&body.reason)?;
    let expires_at = strike_expiry(body.duration_days)?.map(|length| Utc::now() + length);
    let audited = Audited::member(&state, actor, &raw).await?;
    let strike = StrikeRepository::new(state.db.pool())
        .issue(
            audited.target.discord_id,
            reason,
            actor.discord_id,
            expires_at,
            None,
        )
        .await
        .context("issue the strike")?
        .ok_or_else(|| audited.target.no_member())?;
    publish(
        &state,
        &BlacklistEvent::StrikeIssued {
            strike_id: strike.id,
        },
    )
    .await;
    audited
        .record(
            "add_strike",
            json!({ "strike_id": strike.id, "reason": reason, "duration_days": body.duration_days }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_strike(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((raw, strike_id)): Path<(String, i64)>,
) -> Result<StatusCode, AdminError> {
    let audited = Audited::member(&state, actor, &raw).await?;
    let member_id = audited.target.member()?;
    let repo = StrikeRepository::new(state.db.pool());
    let belongs = repo
        .get(strike_id)
        .await
        .context("read the strike")?
        .is_some_and(|strike| strike.member_id == member_id);
    if !belongs {
        return Err(AdminError::NotFound(format!(
            "strike #{strike_id} does not belong to this account"
        )));
    }
    let revoked = repo
        .revoke(strike_id, actor.discord_id)
        .await
        .context("revoke the strike")?;
    if revoked.is_none() {
        return Err(AdminError::Rejected(format!(
            "strike #{strike_id} is already revoked"
        )));
    }
    publish(&state, &BlacklistEvent::StrikeRevoked { strike_id }).await;
    audited
        .record("revoke_strike", json!({ "strike_id": strike_id }))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn regenerate_api_key(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
) -> Result<Json<NewApiKey>, AdminError> {
    let audited = Audited::member(&state, actor, &raw).await?;
    let api_key = uuid::Uuid::new_v4().to_string();
    MemberRepository::new(state.db.pool())
        .set_api_key(audited.target.discord_id, &api_key)
        .await
        .context("store the regenerated key")?;
    audited.record("regenerate_api_key", json!({})).await;
    Ok(Json(NewApiKey { api_key }))
}

async fn create_dev_key(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<DevKeyBody>, JsonRejection>,
) -> Result<Json<DevKey>, AdminError> {
    let Json(body) = body?;
    let label = dev_key_label(&body.label)?;
    let permissions = dev_key_permissions(body.permissions)?;
    let rate_limit = dev_key_rate_limit(body.rate_limit)?;
    let audited = Audited::member(&state, actor, &raw).await?;
    let member_id = audited.target.member()?;
    if developer_key(&state, member_id).await?.is_some() {
        return Err(AdminError::Rejected(
            "this account already has a developer key; delete it first".into(),
        ));
    }
    DeveloperKeyRepository::new(state.db.pool())
        .create(
            member_id,
            &uuid::Uuid::new_v4().to_string(),
            label,
            permissions,
            rate_limit,
        )
        .await
        .context("create the developer key")?;
    audited
        .record(
            "create_dev_key",
            json!({ "label": label, "permissions": permissions, "rate_limit": rate_limit }),
        )
        .await;
    let created = developer_key(&state, member_id).await?.ok_or_else(|| {
        AdminError::Internal("the new developer key could not be read back".into())
    })?;
    Ok(Json(created))
}

async fn delete_dev_key(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
) -> Result<StatusCode, AdminError> {
    let audited = Audited::member(&state, actor, &raw).await?;
    let deleted = DeveloperKeyRepository::new(state.db.pool())
        .delete(audited.target.member()?)
        .await
        .context("delete the developer key")?;
    if !deleted {
        return Err(audited.target.no_developer_key());
    }
    audited.record("delete_dev_key", json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_dev_key_locked(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<LockedBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let audited = Audited::member(&state, actor, &raw).await?;
    let updated = DeveloperKeyRepository::new(state.db.pool())
        .set_locked(audited.target.member()?, body.locked)
        .await
        .context("lock or unlock the developer key")?;
    if !updated {
        return Err(audited.target.no_developer_key());
    }
    audited
        .record("set_dev_key_locked", json!({ "locked": body.locked }))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_dev_key_rate_limit(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<RateLimitBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let rate_limit = dev_key_rate_limit(body.rate_limit)?;
    let audited = Audited::member(&state, actor, &raw).await?;
    let updated = DeveloperKeyRepository::new(state.db.pool())
        .set_rate_limit(audited.target.member()?, rate_limit)
        .await
        .context("set the developer key's rate limit")?;
    if !updated {
        return Err(audited.target.no_developer_key());
    }
    audited
        .record(
            "set_dev_key_rate_limit",
            json!({ "rate_limit": rate_limit }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_dev_key_permissions(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<PermissionsBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let permissions = dev_key_permissions(body.permissions)?;
    let audited = Audited::member(&state, actor, &raw).await?;
    let updated = DeveloperKeyRepository::new(state.db.pool())
        .set_permissions(audited.target.member()?, permissions)
        .await
        .context("set the developer key's permissions")?;
    if !updated {
        return Err(audited.target.no_developer_key());
    }
    audited
        .record(
            "set_dev_key_permissions",
            json!({ "permissions": permissions }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_limits(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
) -> Result<Json<ResetLimits>, AdminError> {
    let target = AccountTarget::resolve(&state, &raw).await?;
    let known = known_limiters(&state, target).await?;
    let keys: Vec<String> = known
        .iter()
        .map(|limiter| coral_redis::key(&limiter.name))
        .collect();
    state
        .redis
        .connection()
        .del::<_, ()>(&keys)
        .await
        .context("clear the account's limiter keys")?;
    let cleared: Vec<Limiter> = known.iter().map(|known| known.limiter).collect();
    let audited = Audited {
        state: &state,
        actor,
        target,
    };
    audited
        .record("reset_rate_limit", json!({ "limiters": cleared }))
        .await;
    Ok(Json(ResetLimits { cleared }))
}

async fn set_license(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
    body: Result<Json<LicenseBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let status = license_status(&body.status)?;
    let audited = Audited::any(&state, actor, &raw).await?;
    let updated = StarfishRepository::new(state.db.pool())
        .set_license_status(audited.target.discord_id, status)
        .await
        .context("set the Starfish license")?;
    if !updated {
        return Err(audited.target.no_starfish_user());
    }
    audited
        .record("set_license_status", json!({ "status": status }))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_sessions(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(raw): Path<String>,
) -> Result<StatusCode, AdminError> {
    let audited = Audited::any(&state, actor, &raw).await?;
    let repo = StarfishRepository::new(state.db.pool());
    let user = repo
        .get_user_by_discord_id(audited.target.discord_id)
        .await
        .context("read the Starfish user")?
        .ok_or_else(|| audited.target.no_starfish_user())?;
    repo.delete_user_sessions(user.id)
        .await
        .context("delete the Starfish sessions")?;
    repo.delete_user_refresh_tokens(user.id)
        .await
        .context("delete the Starfish refresh tokens")?;
    audited.record("revoke_starfish_sessions", json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

impl<'a> Audited<'a> {
    async fn any(state: &'a AppState, actor: AdminActor, raw: &str) -> Result<Self, AdminError> {
        Ok(Self {
            state,
            actor,
            target: AccountTarget::resolve(state, raw).await?,
        })
    }

    async fn member(state: &'a AppState, actor: AdminActor, raw: &str) -> Result<Self, AdminError> {
        let audited = Self::any(state, actor, raw).await?;
        audited.target.member()?;
        Ok(audited)
    }

    async fn record(&self, action: &str, details: Value) {
        audit::log(
            self.state,
            self.actor.discord_id,
            action,
            &self.target.discord_id.to_string(),
            details,
        )
        .await;
    }
}

impl AccountTarget {
    fn member(&self) -> Result<i64, AdminError> {
        self.member_id.ok_or_else(|| self.no_member())
    }

    fn no_member(&self) -> AdminError {
        AdminError::NotFound(format!(
            "Discord user {} has no Coral account",
            self.discord_id
        ))
    }

    fn no_developer_key(&self) -> AdminError {
        AdminError::NotFound(format!(
            "Discord user {} has no developer key",
            self.discord_id
        ))
    }

    fn no_starfish_user(&self) -> AdminError {
        AdminError::NotFound(format!(
            "Discord user {} has no Starfish account",
            self.discord_id
        ))
    }
}

async fn publish(state: &AppState, event: &BlacklistEvent) {
    EventPublisher::new(state.redis.clone())
        .publish(event)
        .await;
}

fn access_level(level: i16) -> Result<i16, AdminError> {
    if AccessRank::from_level(level).to_level() != level {
        return Err(AdminError::BadRequest(format!(
            "{level} is not an access level"
        )));
    }
    Ok(level)
}

fn strike_reason(raw: &str) -> Result<&str, AdminError> {
    let reason = raw.trim();
    if reason.is_empty() || reason.chars().count() > STRIKE_REASON_MAX_CHARS {
        return Err(AdminError::BadRequest(format!(
            "a strike reason needs 1 to {STRIKE_REASON_MAX_CHARS} characters"
        )));
    }
    Ok(reason)
}

fn strike_expiry(days: Option<i64>) -> Result<Option<TimeDelta>, AdminError> {
    match days {
        None => Ok(None),
        Some(days) if STRIKE_DURATION_DAYS.contains(&days) => Ok(Some(TimeDelta::days(days))),
        Some(days) => Err(AdminError::BadRequest(format!(
            "a strike lasts {STRIKE_DURATION_DAYS:?} days or has no expiry, not {days} days"
        ))),
    }
}

fn dev_key_label(raw: &str) -> Result<&str, AdminError> {
    let label = raw.trim();
    if label.is_empty() || label.chars().count() > DEV_KEY_LABEL_MAX_CHARS {
        return Err(AdminError::BadRequest(format!(
            "a developer key label needs 1 to {DEV_KEY_LABEL_MAX_CHARS} characters"
        )));
    }
    Ok(label)
}

fn dev_key_permissions(bits: i64) -> Result<i64, AdminError> {
    let known = permissions::ALL.iter().fold(0, |all, bit| all | bit);
    if bits & !known != 0 {
        return Err(AdminError::BadRequest(format!(
            "{bits} sets permission bits that do not exist"
        )));
    }
    Ok(bits)
}

fn dev_key_rate_limit(limit: i32) -> Result<i32, AdminError> {
    if !(1..=DEV_KEY_MAX_RATE_LIMIT).contains(&limit) {
        return Err(AdminError::BadRequest(format!(
            "a developer key rate limit is 1 to {DEV_KEY_MAX_RATE_LIMIT} requests per 5 minutes"
        )));
    }
    Ok(limit)
}

fn license_status(raw: &str) -> Result<&'static str, AdminError> {
    LICENSE_STATUSES
        .into_iter()
        .find(|status| *status == raw)
        .ok_or_else(|| {
            AdminError::BadRequest(format!(
                "{raw:?} is not a license status; use one of {LICENSE_STATUSES:?}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_access_levels_are_accepted() {
        for level in [0, 2, 3, 4, 5] {
            assert_eq!(access_level(level).unwrap(), level);
        }
        for level in [-1, 1, 6, 42] {
            assert_eq!(access_level(level).unwrap_err().code(), "bad_request");
        }
    }

    #[test]
    fn strikes_need_a_short_reason_and_a_known_length() {
        assert_eq!(strike_reason("  scraping  ").unwrap(), "scraping");
        assert!(strike_reason("   ").is_err());
        assert!(strike_reason(&"x".repeat(STRIKE_REASON_MAX_CHARS + 1)).is_err());
        assert_eq!(strike_expiry(None).unwrap(), None);
        assert_eq!(strike_expiry(Some(14)).unwrap(), Some(TimeDelta::days(14)));
        assert!(strike_expiry(Some(7)).is_err());
    }

    #[test]
    fn developer_key_settings_are_bounded() {
        assert_eq!(
            dev_key_permissions(permissions::PLAYER_DATA | permissions::HYPIXEL).unwrap(),
            3
        );
        assert!(dev_key_permissions(1 << 10).is_err());
        assert!(dev_key_rate_limit(0).is_err());
        assert_eq!(dev_key_rate_limit(900).unwrap(), 900);
        assert!(dev_key_label("").is_err());
    }

    #[test]
    fn license_statuses_are_the_three_starfish_knows() {
        assert_eq!(license_status("suspended").unwrap(), "suspended");
        assert_eq!(license_status("revoked").unwrap_err().code(), "bad_request");
    }
}
