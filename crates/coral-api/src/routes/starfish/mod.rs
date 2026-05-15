pub mod auth;
mod download;
mod license;
mod plugins;
pub mod session_auth;
mod users;

use std::sync::Arc;

use axum::Router;
use coral_redis::RateLimitResult;
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, state::{AppState, StarfishConfig}};


pub(crate) fn require_starfish(state: &AppState) -> Result<Arc<StarfishConfig>, ApiError> {
    state.starfish.clone().ok_or_else(|| ApiError::ServiceUnavailable("Starfish not configured".into()))
}


pub(crate) async fn rate_limit(state: &AppState, key: &str, limit: i64) -> Result<(), ApiError> {
    match state.rate_limiter.check_and_record(key, limit).await {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(ApiError::RateLimited),
        Err(_) => Ok(()),
    }
}


pub(crate) fn is_owner(discord_id: i64) -> bool {
    static OWNERS: std::sync::OnceLock<std::collections::HashSet<i64>> = std::sync::OnceLock::new();
    OWNERS.get_or_init(|| {
        std::env::var("OWNER_IDS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect()
    }).contains(&discord_id)
}


pub(crate) fn require_owner(caller: &session_auth::AuthenticatedStarfishUser) -> Result<(), ApiError> {
    if is_owner(caller.user.discord_id) {
        Ok(())
    } else {
        Err(ApiError::Forbidden("owner_only".into()))
    }
}


pub fn router(state: AppState) -> Router<AppState> {
    if state.starfish.is_none() {
        tracing::info!("Starfish routes disabled (no config)");
        return Router::new();
    }

    Router::new()
        .merge(auth::router())
        .merge(license::router())
        .merge(download::router())
        .merge(users::router(state.clone()))
        .merge(plugins::router(state))
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreTables {
    pub pkt_chat: i32,
    pub pkt_entity_equipment: i32,
    pub pkt_update_health: i32,
    pub pkt_respawn: i32,
    pub pkt_player_position_and_look: i32,
    pub pkt_held_item_change: i32,
    pub pkt_animation: i32,
    pub pkt_spawn_player: i32,
    pub pkt_destroy_entities: i32,
    pub pkt_entity_relative_move: i32,
    pub pkt_entity_look: i32,
    pub pkt_entity_look_and_relative_move: i32,
    pub pkt_entity_teleport: i32,
    pub pkt_entity_status: i32,
    pub pkt_entity_metadata: i32,
    pub pkt_set_experience: i32,
    pub pkt_chunk_data: i32,
    pub pkt_multi_block_change: i32,
    pub pkt_block_change: i32,
    pub pkt_block_break_animation: i32,
    pub pkt_map_chunk_bulk: i32,
    pub pkt_open_window: i32,
    pub pkt_close_window: i32,
    pub pkt_set_slot: i32,
    pub pkt_window_items: i32,
    pub pkt_player_list_item: i32,
    pub pkt_scoreboard_objective: i32,
    pub pkt_update_score: i32,
    pub pkt_display_scoreboard: i32,
    pub pkt_teams: i32,
    pub pkt_player_list_header_footer: i32,
    pub pkt_plugin_message: i32,
    pub coord_scale: f64,
    pub rotation_scale: f32,
    pub player_entity_type: u8,
    pub player_list_action_add: i32,
    pub player_list_action_update_gamemode: i32,
    pub player_list_action_update_ping: i32,
    pub player_list_action_update_display_name: i32,
    pub player_list_action_remove: i32,
    pub team_mode_create: i8,
    pub team_mode_remove: i8,
    pub team_mode_update: i8,
    pub team_mode_add_players: i8,
    pub team_mode_remove_players: i8,
}


impl Default for CoreTables {
    fn default() -> Self {
        Self {
            pkt_chat: 0x02, pkt_entity_equipment: 0x04, pkt_update_health: 0x06,
            pkt_respawn: 0x07, pkt_player_position_and_look: 0x08, pkt_held_item_change: 0x09,
            pkt_animation: 0x0B, pkt_spawn_player: 0x0C, pkt_destroy_entities: 0x13,
            pkt_entity_relative_move: 0x15, pkt_entity_look: 0x16,
            pkt_entity_look_and_relative_move: 0x17, pkt_entity_teleport: 0x18,
            pkt_entity_status: 0x1A, pkt_entity_metadata: 0x1C, pkt_set_experience: 0x1F,
            pkt_chunk_data: 0x21, pkt_multi_block_change: 0x22, pkt_block_change: 0x23,
            pkt_block_break_animation: 0x25, pkt_map_chunk_bulk: 0x26, pkt_open_window: 0x2D,
            pkt_close_window: 0x2E, pkt_set_slot: 0x2F, pkt_window_items: 0x30,
            pkt_player_list_item: 0x38, pkt_scoreboard_objective: 0x3B,
            pkt_update_score: 0x3C, pkt_display_scoreboard: 0x3D, pkt_teams: 0x3E,
            pkt_player_list_header_footer: 0x47, pkt_plugin_message: 0x3F,
            coord_scale: 32.0, rotation_scale: 360.0 / 256.0, player_entity_type: 0,
            player_list_action_add: 0, player_list_action_update_gamemode: 1,
            player_list_action_update_ping: 2, player_list_action_update_display_name: 3,
            player_list_action_remove: 4,
            team_mode_create: 0, team_mode_remove: 1, team_mode_update: 2,
            team_mode_add_players: 3, team_mode_remove_players: 4,
        }
    }
}


pub fn default_core_tables_bytes() -> Vec<u8> {
    bincode::serialize(&CoreTables::default()).expect("Failed to serialize default CoreTables")
}
