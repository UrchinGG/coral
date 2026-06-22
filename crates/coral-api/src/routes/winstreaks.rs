use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use database::CacheRepository;
use hypixel::parsing::winstreaks;
use hypixel::{Mode, extract_winstreak_snapshot};

use crate::{
    error::ApiError,
    routes::{player, session},
    state::AppState,
};

const MODE_GROUPS: [(&str, &[Mode]); 7] = [
    (
        "overall",
        &[
            Mode::Solos,
            Mode::Doubles,
            Mode::Threes,
            Mode::Fours,
            Mode::FourVFour,
        ],
    ),
    (
        "core",
        &[Mode::Solos, Mode::Doubles, Mode::Threes, Mode::Fours],
    ),
    ("solos", &[Mode::Solos]),
    ("doubles", &[Mode::Doubles]),
    ("threes", &[Mode::Threes]),
    ("fours", &[Mode::Fours]),
    ("4v4", &[Mode::FourVFour]),
];

pub fn router() -> Router<AppState> {
    Router::new().route("/player/winstreaks", get(player_winstreaks))
}

#[derive(Serialize, ToSchema)]
pub struct WinstreakResponse {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[schema(value_type = HashMap<String, Vec<StreakEntry>>)]
    pub modes: HashMap<String, Vec<StreakEntry>>,
}

#[derive(Serialize, ToSchema)]
pub struct StreakEntry {
    pub value: u64,
    pub approximate: bool,
    pub timestamp: i64,
    pub readable: String,
}

#[utoipa::path(
    get,
    path = "/v3/player/winstreaks",
    description = "Reconstructs a player's Bedwars winstreak history for each mode from stored snapshots. An entry marked `approximate` was inferred where the snapshot history contains a gap.",
    params(session::PlayerQuery),
    responses(
        (status = 200, body = WinstreakResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Player",
    security(("api_key" = []))
)]
pub async fn player_winstreaks(
    State(state): State<AppState>,
    Query(query): Query<session::PlayerQuery>,
) -> Result<Json<WinstreakResponse>, ApiError> {
    let (uuid, _) = player::resolve_identifier(&state, &query.player).await?;

    let snapshots = CacheRepository::new(state.db.pool())
        .get_all_snapshots_mapped(&uuid, extract_winstreak_snapshot)
        .await?;

    let modes = MODE_GROUPS
        .into_iter()
        .map(|(key, group)| {
            let streaks = winstreaks::calculate(&snapshots, group)
                .streaks
                .into_iter()
                .map(|s| StreakEntry {
                    value: s.value,
                    approximate: s.approximate,
                    timestamp: s.timestamp.timestamp_millis(),
                    readable: s.timestamp.format("%b %d, %Y %H:%M UTC").to_string(),
                })
                .collect();
            (key.to_string(), streaks)
        })
        .collect();

    let displayname = player::cached_display_name(&state, &uuid).await;
    Ok(Json(WinstreakResponse {
        uuid,
        displayname,
        modes,
    }))
}
