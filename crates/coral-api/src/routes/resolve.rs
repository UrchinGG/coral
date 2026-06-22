use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use database::permissions;

use crate::auth::DeveloperKeyAuth;
use crate::error::ApiError;
use crate::routes::player::player_display_name;
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct ResolveResponse {
    pub uuid: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/resolve/{identifier}", get(resolve_player))
}

#[utoipa::path(
    get,
    path = "/v3/resolve/{identifier}",
    description = "Resolves a UUID or username to the canonical UUID and username through Mojang. Requires the `Player Data` permission or an Admin key.",
    params(
        ("identifier" = String, Path, description = "Player UUID or username")
    ),
    responses(
        (status = 200, description = "Player resolved", body = ResolveResponse),
        (status = 400, description = "Invalid identifier", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Player not found", body = crate::error::ErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::ErrorResponse),
    ),
    tag = "Internal",
    security(("api_key" = []))
)]
pub async fn resolve_player(
    State(state): State<AppState>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Path(identifier): Path<String>,
) -> Result<Json<ResolveResponse>, ApiError> {
    if let Some(Extension(ref dev)) = dev_auth {
        dev.require(permissions::PLAYER_DATA)?;
    }
    let id = state.mojang.resolve(&identifier).await?;
    let displayname = player_display_name(&state, &id.uuid).await;
    Ok(Json(ResolveResponse {
        uuid: id.uuid,
        username: id.username,
        displayname,
    }))
}
