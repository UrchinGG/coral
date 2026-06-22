use std::collections::HashMap;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use clients::{is_uuid, normalize_uuid};
use database::BlacklistRepository;

use crate::{
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

#[utoipa::path(
    post,
    path = "/v3/players",
    description = "Looks up blacklist tags for up to 100 players in a single request. Only UUIDs are accepted; usernames are not resolved, and malformed entries are skipped. Each queried player also triggers a background snapshot refresh to help saturate the player cache.",
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

    for uuid in &uuids {
        let (state, uuid) = (state.clone(), uuid.clone());
        tokio::spawn(async move {
            refresh_player_cache(&state, &uuid, None).await;
        });
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
