use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use database::GuildConfigRepository;
use serde::Serialize;

use super::directory::{icon_url, sync_http};
use crate::error::{AdminError, Context};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SyncServer {
    id: String,
    name: String,
    icon_url: Option<String>,
    enabled: bool,
}

/// Every server the sync bot is in, and whether sync is on there.
pub async fn servers(State(state): State<AppState>) -> Result<Json<Vec<SyncServer>>, AdminError> {
    // shortcut: reads one page of the bot's servers (200); page through them past that.
    let guilds = sync_http(&state)?
        .get_guilds(None, None)
        .await
        .map_err(|error| {
            AdminError::Upstream(format!("Discord: list the bot's servers: {error}"))
        })?;
    let enabled: HashSet<i64> = GuildConfigRepository::new(state.db.pool())
        .get_all()
        .await
        .context("read which servers have sync on")?
        .into_iter()
        .map(|config| config.guild_id)
        .collect();
    Ok(Json(
        guilds
            .into_iter()
            .map(|guild| SyncServer {
                id: guild.id.to_string(),
                name: guild.name.to_string(),
                icon_url: icon_url(guild.id, guild.icon),
                enabled: enabled.contains(&(guild.id.get() as i64)),
            })
            .collect(),
    ))
}
