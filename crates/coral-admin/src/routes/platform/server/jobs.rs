use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use reqwest::Method;
use serde_json::Value;

use super::{Server, change, sync_settings};
use crate::auth::AdminActor;
use crate::error::AdminError;
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    Server(guild_id): Server,
) -> Result<Json<Value>, AdminError> {
    let path = format!("/guilds/{guild_id}/jobs");
    Ok(Json(sync_settings(&state)?.read(&path).await?))
}

pub async fn cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    Path((_, job_id)): Path<(u64, i64)>,
) -> Result<StatusCode, AdminError> {
    let path = format!("/jobs/{job_id}");
    change(
        &state,
        &actor,
        guild_id,
        Method::DELETE,
        &path,
        None,
        "cancel_sync_job",
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}
