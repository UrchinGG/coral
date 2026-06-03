use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::Deserialize;
use utoipa::ToSchema;

use clients::normalize_uuid;
use coral_redis::{BlacklistEvent, RateLimitResult};
use database::{TagOp, TagOpError};

use crate::{
    auth::AuthenticatedMember, cache::refresh_player_cache, error::ApiError,
    responses::TagResponse, state::AppState,
};

const MAX_REASON_LENGTH: usize = 500;
const MAX_UUID_LENGTH: usize = 36;

#[derive(Deserialize, ToSchema)]
pub(crate) struct UuidQuery {
    pub uuid: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct AddTagBody {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub reason: String,
    #[serde(default)]
    pub hide_username: bool,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct RemoveTagBody {
    #[serde(rename = "type")]
    pub tag_type: String,
}

/// Overwrites an existing tag. `tag_type` identifies the existing tag (must currently be active).
/// `new_type` may equal `tag_type` to replace just the reason; otherwise it switches the tag type.
#[derive(Deserialize, ToSchema)]
pub(crate) struct UpdateTagBody {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub new_type: String,
    pub new_reason: String,
    #[serde(default)]
    pub hide_username: bool,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct LockRequest {
    pub reason: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/tags", post(add_tag).delete(remove_tag).patch(update_tag))
}

pub fn mod_router() -> Router<AppState> {
    Router::new().route("/player/lock", post(lock_player).delete(unlock_player))
}

fn validate_uuid(uuid: &str) -> Result<String, ApiError> {
    if uuid.len() > MAX_UUID_LENGTH {
        return Err(ApiError::BadRequest("uuid too long".into()));
    }
    Ok(normalize_uuid(uuid))
}

fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if reason.len() > MAX_REASON_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "reason exceeds maximum length of {MAX_REASON_LENGTH} characters"
        )));
    }
    Ok(())
}

fn map_op_error(e: TagOpError) -> ApiError {
    match e {
        TagOpError::PlayerLocked => ApiError::Forbidden("player is locked".into()),
        TagOpError::InsufficientPermissions => {
            ApiError::Forbidden("insufficient permissions".into())
        }
        TagOpError::InvalidTagType => ApiError::BadRequest("invalid tag type".into()),
        TagOpError::TagAlreadyExists => {
            ApiError::Conflict("player already has this tag type".into())
        }
        TagOpError::PriorityConflict(t) => ApiError::Conflict(format!(
            "conflicts with existing '{}' tag",
            t.tag_type.as_deref().unwrap_or("")
        )),
        TagOpError::TagNotFound => ApiError::NotFound("tag not found".into()),
        TagOpError::EditWindowExpired => ApiError::Forbidden("edit window has expired".into()),
        TagOpError::ModeratorRequired => ApiError::Forbidden("moderator access required".into()),
        TagOpError::Database(e) => ApiError::Internal(format!("database error: {e}")),
    }
}

async fn enforce_tag_limit(state: &AppState, member: &database::Member) -> Result<(), ApiError> {
    match state
        .rate_limiter
        .check_tag_limit(member.discord_id, member.access_level)
        .await
    {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(ApiError::RateLimited),
        Err(_) => Err(ApiError::Internal("rate limit check failed".into())),
    }
}

#[utoipa::path(
    post, path = "/v3/tags",
    params(("uuid" = String, Query)),
    request_body = AddTagBody,
    responses(
        (status = 201, description = "Tag added", body = TagResponse),
        (status = 400, body = crate::error::ErrorResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 409, body = crate::error::ErrorResponse),
    ),
    tag = "Blacklist", security(("api_key" = []))
)]
pub async fn add_tag(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    Query(query): Query<UuidQuery>,
    Json(body): Json<AddTagBody>,
) -> Result<(StatusCode, Json<TagResponse>), ApiError> {
    if member.0.tagging_disabled {
        return Err(ApiError::Forbidden(
            "tagging is disabled on your account".into(),
        ));
    }
    enforce_tag_limit(&state, &member.0).await?;
    validate_reason(&body.reason)?;

    let uuid = validate_uuid(&query.uuid)?;
    let ops = TagOp::new(state.db.pool());

    let tag = ops
        .add(
            &uuid,
            &body.tag_type,
            &body.reason,
            member.0.discord_id,
            member.0.access_level,
            body.hide_username,
            None,
            None,
        )
        .await
        .map_err(map_op_error)?;

    state
        .event_publisher
        .publish(&BlacklistEvent::TagAdded {
            uuid: uuid.clone(),
            tag_id: tag.id,
            added_by: member.0.discord_id,
        })
        .await;

    let state = state.clone();
    tokio::spawn(async move { refresh_player_cache(&state, &uuid, None).await });

    Ok((StatusCode::CREATED, Json(TagResponse::from_db(&tag))))
}

#[utoipa::path(
    delete, path = "/v3/tags",
    params(("uuid" = String, Query)),
    request_body = RemoveTagBody,
    responses(
        (status = 204, description = "Tag removed"),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Blacklist", security(("api_key" = []))
)]
pub async fn remove_tag(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    Query(query): Query<UuidQuery>,
    Json(body): Json<RemoveTagBody>,
) -> Result<StatusCode, ApiError> {
    enforce_tag_limit(&state, &member.0).await?;

    let uuid = validate_uuid(&query.uuid)?;
    let ops = TagOp::new(state.db.pool());

    let tag = ops
        .remove(
            &uuid,
            &body.tag_type,
            member.0.discord_id,
            member.0.access_level,
        )
        .await
        .map_err(map_op_error)?;

    state
        .event_publisher
        .publish(&BlacklistEvent::TagRemoved {
            uuid: uuid.clone(),
            tag_id: tag.id,
            removed_by: member.0.discord_id,
        })
        .await;

    let state = state.clone();
    tokio::spawn(async move { refresh_player_cache(&state, &uuid, None).await });

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    patch, path = "/v3/tags",
    params(("uuid" = String, Query)),
    request_body = UpdateTagBody,
    responses(
        (status = 200, description = "Tag updated", body = TagResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Blacklist", security(("api_key" = []))
)]
pub async fn update_tag(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    Query(query): Query<UuidQuery>,
    Json(body): Json<UpdateTagBody>,
) -> Result<Json<TagResponse>, ApiError> {
    if member.0.tagging_disabled {
        return Err(ApiError::Forbidden(
            "tagging is disabled on your account".into(),
        ));
    }
    if body.new_type == "confirmed_cheater" {
        return Err(ApiError::Forbidden(
            "confirmed cheater tags can only be applied through the review system".into(),
        ));
    }
    validate_reason(&body.new_reason)?;
    enforce_tag_limit(&state, &member.0).await?;

    let uuid = validate_uuid(&query.uuid)?;
    let ops = TagOp::new(state.db.pool());

    let (old_tag, new_tag) = ops
        .overwrite(
            &uuid,
            &body.tag_type,
            &body.new_type,
            &body.new_reason,
            member.0.discord_id,
            member.0.access_level,
            body.hide_username,
        )
        .await
        .map_err(map_op_error)?;

    state
        .event_publisher
        .publish(&BlacklistEvent::TagOverwritten {
            uuid: uuid.clone(),
            old_tag_id: old_tag.id,
            old_tag_type: old_tag.tag_type.clone().unwrap_or_default(),
            old_reason: old_tag.reason.clone().unwrap_or_default(),
            new_tag_id: new_tag.id,
            overwritten_by: member.0.discord_id,
        })
        .await;

    let state = state.clone();
    tokio::spawn(async move { refresh_player_cache(&state, &uuid, None).await });

    Ok(Json(TagResponse::from_db(&new_tag)))
}

#[utoipa::path(
    post, path = "/v3/player/lock",
    params(("uuid" = String, Query)),
    request_body = LockRequest,
    responses(
        (status = 204, description = "Player locked"),
        (status = 403, body = crate::error::ErrorResponse),
    ),
    tag = "Blacklist", security(("api_key" = []))
)]
pub async fn lock_player(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    Query(query): Query<UuidQuery>,
    Json(req): Json<LockRequest>,
) -> Result<StatusCode, ApiError> {
    validate_reason(&req.reason)?;

    let uuid = validate_uuid(&query.uuid)?;
    let ops = TagOp::new(state.db.pool());

    ops.lock_player(
        &uuid,
        &req.reason,
        member.0.discord_id,
        member.0.access_level,
    )
    .await
    .map_err(map_op_error)?;

    state
        .event_publisher
        .publish(&BlacklistEvent::PlayerLocked {
            uuid,
            locked_by: member.0.discord_id,
            reason: req.reason,
        })
        .await;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete, path = "/v3/player/lock",
    params(("uuid" = String, Query)),
    responses(
        (status = 204, description = "Player unlocked"),
        (status = 403, body = crate::error::ErrorResponse),
    ),
    tag = "Blacklist", security(("api_key" = []))
)]
pub async fn unlock_player(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    Query(query): Query<UuidQuery>,
) -> Result<StatusCode, ApiError> {
    let uuid = validate_uuid(&query.uuid)?;

    TagOp::new(state.db.pool())
        .unlock_player(&uuid, member.0.discord_id, member.0.access_level)
        .await
        .map_err(map_op_error)?;

    state
        .event_publisher
        .publish(&BlacklistEvent::PlayerUnlocked {
            uuid,
            unlocked_by: member.0.discord_id,
        })
        .await;

    Ok(StatusCode::NO_CONTENT)
}
