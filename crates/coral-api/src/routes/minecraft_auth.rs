use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use coral_redis::minecraft_session::{self, MinecraftUser};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{auth::caller_rate_limit, error::ApiError, reqlog::client_ip, state::AppState};

const CHALLENGES_PER_IP: i64 = 30;

#[derive(Serialize, ToSchema)]
pub struct ChallengeResponse {
    pub server_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct SignInRequest {
    pub username: String,
    pub server_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct SignInResponse {
    pub token: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/minecraft/challenge", post(challenge))
        .route("/auth/minecraft", post(sign_in))
}

#[utoipa::path(
    post,
    path = "/v3/auth/minecraft/challenge",
    responses(
        (status = 200, description = "Challenge issued; pass `server_id` to Mojang's `joinServer` within a minute", body = ChallengeResponse),
        (status = 429, description = "Too many challenges from this address", body = crate::error::ErrorResponse),
    ),
    tag = "Authentication",
)]
pub async fn challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ChallengeResponse>, ApiError> {
    caller_rate_limit(
        &state,
        &format!(
            "mcchallenge:{}",
            client_ip(&headers).map_or_else(|| "unknown".into(), |ip| ip.to_string())
        ),
        CHALLENGES_PER_IP,
    )
    .await?;
    Ok(Json(ChallengeResponse {
        server_id: minecraft_session::issue_challenge(&state.redis)
            .await
            .map_err(redis_failure)?,
    }))
}

#[utoipa::path(
    post,
    path = "/v3/auth/minecraft",
    request_body = SignInRequest,
    responses(
        (status = 200, description = "Signed in; send `token` as `Authorization: Bearer <token>` for 24 hours", body = SignInResponse),
        (status = 401, description = "The challenge is unknown, expired, or Mojang has no record of the join", body = crate::error::ErrorResponse),
        (status = 502, description = "Mojang could not be reached", body = crate::error::ErrorResponse),
    ),
    tag = "Authentication",
)]
pub async fn sign_in(
    State(state): State<AppState>,
    Json(body): Json<SignInRequest>,
) -> Result<Json<SignInResponse>, ApiError> {
    if !minecraft_session::redeem_challenge(&state.redis, &body.server_id)
        .await
        .map_err(redis_failure)?
    {
        return Err(ApiError::Unauthorized(
            "unknown or expired challenge".into(),
        ));
    }

    let player = state
        .mojang
        .has_joined(&body.username, &body.server_id)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("Mojang has no record of this join".into()))?;
    Ok(Json(SignInResponse {
        token: minecraft_session::start(
            &state.redis,
            &MinecraftUser {
                uuid: Uuid::parse_str(&player.uuid).map_err(|_| {
                    ApiError::Upstream(format!("Mojang returned an invalid UUID: {}", player.uuid))
                })?,
                username: player.username,
            },
        )
        .await
        .map_err(redis_failure)?,
    }))
}

fn redis_failure(error: redis::RedisError) -> ApiError {
    ApiError::Internal(format!("redis: {error}"))
}
