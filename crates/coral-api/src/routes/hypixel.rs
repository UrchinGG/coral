use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use clients::{is_uuid, normalize_uuid};

use crate::{error::ApiError, state::AppState};

use super::player::resolve_identifier;


#[derive(Deserialize)]
pub(crate) struct PlayerQuery {
    pub uuid: Option<String>,
    pub name: Option<String>,
    pub key: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GuildQuery {
    pub player: Option<String>,
    pub name: Option<String>,
    pub key: Option<String>,
}


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/hypixel/player", get(player))
        .route("/hypixel/guild", get(guild))
}


async fn player(
    State(state): State<AppState>,
    Query(query): Query<PlayerQuery>,
) -> Result<Json<Value>, ApiError> {
    let identifier = query.uuid.as_deref().or(query.name.as_deref())
        .ok_or_else(|| ApiError::BadRequest("query parameter 'uuid' or 'name' required".into()))?;
    let (uuid, _) = resolve_identifier(&state, identifier).await?;
    let data = state.hypixel.get_player(&uuid).await?;
    Ok(Json(json!({ "success": true, "player": data })))
}


async fn guild(
    State(state): State<AppState>,
    Query(query): Query<GuildQuery>,
) -> Result<Json<Value>, ApiError> {
    let data = match (query.player.as_deref(), query.name.as_deref()) {
        (Some(player), _) => {
            let uuid = if is_uuid(player) {
                normalize_uuid(player)
            } else {
                let id = state.mojang.resolve(player).await?;
                normalize_uuid(&id.uuid)
            };
            state.hypixel.get_guild_by_player(&uuid).await?
        }
        (_, Some(name)) => state.hypixel.get_guild_by_name(name).await?,
        _ => return Err(ApiError::BadRequest("query parameter 'player' or 'name' required".into())),
    };
    Ok(Json(json!({ "success": true, "guild": data })))
}
