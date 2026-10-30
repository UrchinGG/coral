use std::collections::HashMap;

use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use clients::{is_uuid, normalize_uuid};
use coral_redis::RateLimitResult;
use database::BlacklistRepository;
use reqlog::MAX_BATCH_UUIDS;

use crate::{
    auth::{
        AuthenticatedMember, DeveloperKeyAuth, KEY_DISTINCT_UUID_BUDGET, MinecraftAuth,
        SESSION_UUID_BUDGET, SessionAuth,
    },
    cache::spawn_background_refresh,
    error::ApiError,
    responses::{TagResponse, tag_responses},
    state::AppState,
};

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

async fn enforce_uuid_budget(state: &AppState, budget: &str, count: usize) -> Result<(), ApiError> {
    match state
        .rate_limiter
        .check_and_record_weighted(budget, SESSION_UUID_BUDGET, count as i64)
        .await
    {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(ApiError::RateLimited),
        Err(_) => Ok(()),
    }
}

async fn enforce_distinct_uuid_budget(
    state: &AppState,
    discord_id: i64,
    uuids: &[String],
) -> Result<(), ApiError> {
    match state
        .rate_limiter
        .admit_distinct(
            &format!("keyuuids:{discord_id}"),
            uuids,
            KEY_DISTINCT_UUID_BUDGET,
        )
        .await
    {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(ApiError::RateLimited),
        Err(e) => Err(ApiError::Internal(format!("uuid budget check failed: {e}"))),
    }
}

#[utoipa::path(
    post,
    path = "/v3/players",
    description = "Looks up blacklist tags for up to 100 players in a single request. Only UUIDs are accepted; usernames are not resolved, and malformed entries are skipped. Personal API keys may query up to 1000 distinct players per 5 minutes; repeat lookups of the same players do not count again. Queried players may also be refreshed in the background. Tags with `hide_username` set omit both `added_by` and `added_by_username`.",
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
    minecraft: Option<Extension<MinecraftAuth>>,
    member: Option<Extension<AuthenticatedMember>>,
    developer: Option<Extension<DeveloperKeyAuth>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, ApiError> {
    if req.uuids.is_empty() {
        return Err(ApiError::BadRequest("uuids array is empty".into()));
    }
    if req.uuids.len() > MAX_BATCH_UUIDS {
        return Err(ApiError::BadRequest(format!(
            "batch size exceeds maximum of {MAX_BATCH_UUIDS}"
        )));
    }

    let uuids: Vec<String> = req
        .uuids
        .iter()
        .filter(|u| is_uuid(u))
        .map(|u| normalize_uuid(u))
        .collect();

    match (&session, &minecraft) {
        (Some(Extension(session)), _) => {
            enforce_uuid_budget(
                &state,
                &format!("sfuuids:{}", session.discord_id),
                uuids.len(),
            )
            .await?
        }
        (_, Some(Extension(minecraft))) => {
            enforce_uuid_budget(
                &state,
                &format!("mcuuids:{}", minecraft.uuid.simple()),
                uuids.len(),
            )
            .await?
        }
        _ => {
            if let (Some(Extension(member)), None) = (&member, &developer) {
                enforce_distinct_uuid_budget(&state, member.0.discord_id, &uuids).await?;
            }
            for uuid in &uuids {
                spawn_background_refresh(&state, uuid);
            }
        }
    }

    let batch = BlacklistRepository::new(state.db.pool())
        .get_players_batch(&uuids)
        .await
        .map_err(|e| ApiError::Internal(format!("batch lookup failed: {e}")))?;

    let mut names = HashMap::new();
    let mut players = HashMap::new();
    for (uuid, tags) in batch {
        players.insert(
            uuid,
            tag_responses(&tags, &state.discord, &mut names, false).await,
        );
    }

    Ok(Json(BatchResponse { players }))
}
