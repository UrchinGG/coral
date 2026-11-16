use std::collections::{HashMap, HashSet};

use axum::extract::State;
use database::GuildConfigRepository;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use serenity::all::{GuildId, Http};

use super::directory::{icon_url, sync_http};
use crate::error::{AdminError, Context};
use crate::state::AppState;
use crate::views::{CachedView, View};

const SERVERS_KEY: &str = "";

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncServer {
    id: String,
    name: String,
    icon_url: Option<String>,
    enabled: bool,
    members: Option<u64>,
}

/// Every server the sync bot is in, whether sync is on there and its approximate member count.
pub async fn servers(State(state): State<AppState>) -> Result<CachedView, AdminError> {
    cached_servers(&state).await
}

/// The name of each server the sync bot is in, by server id.
pub async fn server_names(state: &AppState) -> Result<HashMap<String, String>, AdminError> {
    let servers: Vec<SyncServer> = cached_servers(state).await?.decode()?;
    Ok(servers
        .into_iter()
        .map(|server| (server.id, server.name))
        .collect())
}

pub async fn forget_servers(state: &AppState) {
    state.views.forget(View::SyncServers, SERVERS_KEY).await;
}

async fn cached_servers(state: &AppState) -> Result<CachedView, AdminError> {
    state
        .views
        .get_or_load(View::SyncServers, SERVERS_KEY, || load_servers(state))
        .await
}

async fn load_servers(state: &AppState) -> Result<Vec<SyncServer>, AdminError> {
    let http = sync_http(state)?;
    let guilds = http.get_guilds(None, None).await.map_err(|error| {
        AdminError::Upstream(format!("Discord: list the bot's servers: {error}"))
    })?;
    let enabled: HashSet<i64> = GuildConfigRepository::new(state.db.pool())
        .get_all()
        .await
        .context("read which servers have sync on")?
        .into_iter()
        .map(|config| config.guild_id)
        .collect();
    let counts = join_all(guilds.iter().map(|guild| member_count(http, guild.id))).await;
    Ok(guilds
        .into_iter()
        .zip(counts)
        .map(|(guild, members)| SyncServer {
            id: guild.id.to_string(),
            name: guild.name.to_string(),
            icon_url: icon_url(guild.id, guild.icon),
            enabled: enabled.contains(&(guild.id.get() as i64)),
            members,
        })
        .collect())
}

async fn member_count(http: &Http, guild_id: GuildId) -> Option<u64> {
    match http.get_guild_with_counts(guild_id).await {
        Ok(guild) => guild.approximate_member_count.map(|count| count.get()),
        Err(error) => {
            tracing::warn!(%guild_id, %error, "Discord: count the server's members");
            None
        }
    }
}
