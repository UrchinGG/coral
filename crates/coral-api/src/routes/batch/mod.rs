use std::collections::HashMap;

use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use clients::{is_uuid, normalize_uuid};
use coral_redis::RateLimitResult;
use database::BlacklistRepository;

use crate::{
    auth::{SESSION_UUID_BUDGET, SessionAuth},
    cache::refresh_player_cache,
    error::ApiError,
    responses::{TagResponse, tag_responses},
    state::AppState,
};

const MAX_BATCH_SIZE: usize = 100;

#[derive(Deserialize, ToSchema)]
pub(crate) struct BatchRequest {
    pub uuids: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct BatchResponse {
    pub players: HashMap<String, Vec<TagResponse>>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/players", post(batch_lookup))
}

async fn enforce_uuid_budget(
    state: &AppState,
    discord_id: i64,
    count: usize,
) -> Result<(), ApiError> {
    match state
        .rate_limiter
        .check_and_record_weighted(
            &format!("sfuuids:{discord_id}"),
            SESSION_UUID_BUDGET,
            count as i64,
        )
        .await
    {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(ApiError::RateLimited),
        Err(_) => Ok(()),
    }
}

#[utoipa::path(
    post,
    path = "/v3/players",
    description = "Looks up blacklist tags for up to 100 players in a single request. Only UUIDs are accepted; usernames are not resolved, and malformed entries are skipped. Each queried player also triggers a background snapshot refresh to help saturate the player cache. Tags with `hide_username` set omit both `added_by` and `added_by_username`.",
    request_body = BatchRequest,
    responses(
        (status = 200, description = "Batch lookup completed", body = BatchResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    tag = "Player",
)]
pub async fn batch_lookup(
    State(state): State<AppState>,
    session: Option<Extension<SessionAuth>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, ApiError> {
    if req.uuids.is_empty() {
        return Err(ApiError::BadRequest("uuids array is empty".into()));
    }
    if req.uuids.len() > MAX_BATCH_SIZE {
        return Err(ApiError::BadRequest(format!(
            "batch size exceeds maximum of {MAX_BATCH_SIZE}"
        )));
    }

    let uuids: Vec<String> = req
        .uuids
        .iter()
        .filter(|u| is_uuid(u))
        .map(|u| normalize_uuid(u))
        .collect();

    if let Some(Extension(session)) = &session {
        enforce_uuid_budget(&state, session.discord_id, uuids.len()).await?;
    } else {
        for uuid in &uuids {
            let (state, uuid) = (state.clone(), uuid.clone());
            tokio::spawn(async move {
                refresh_player_cache(&state, &uuid, None).await;
            });
        }
    }

    let batch = BlacklistRepository::new(state.db.pool())
        .get_players_batch(&uuids)
        .await
        .map_err(|e| ApiError::Internal(format!("batch lookup failed: {e}")))?;

    let mut names = HashMap::new();
    let mut players = HashMap::new();
    for (uuid, tags) in batch {
        players.insert(uuid, tag_responses(&tags, &state.discord, &mut names).await);
    }

    Ok(Json(BatchResponse { players }))
}
