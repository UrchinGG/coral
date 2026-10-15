use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::error::{AdminError, Context};
use crate::refs::Labels;
use crate::serde_id;
use crate::state::AppState;

mod activity;
mod exact_json;
mod list;
mod lookups;
mod mutations;
pub mod network;
mod profile;
mod rail;
mod requests;
mod starfish;
#[cfg(test)]
pub mod test_db;
mod timeline;
mod windows;

pub const FIRST_ERROR_STATUS: i16 = 400;
const SNOWFLAKE_DIGITS: std::ops::RangeInclusive<usize> = 17..=20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountTarget {
    pub discord_id: i64,
    pub member_id: Option<i64>,
}

#[derive(Serialize)]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub labels: Labels,
}

#[derive(Serialize)]
struct DiscordIdOf {
    #[serde(serialize_with = "serde_id::discord_id")]
    discord_id: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list::accounts))
        .route("/by-member/{member_id}", get(by_member))
        .route("/by-starfish/{user_id}", get(by_starfish))
        .route("/{discord_id}", get(profile::account))
        .route("/{discord_id}/activity", get(activity::activity))
        .route("/{discord_id}/requests", get(requests::requests))
        .route("/{discord_id}/network", get(network::network))
        .route("/{discord_id}/lookups/summary", get(lookups::summary))
        .route("/{discord_id}/lookups/top", get(lookups::top))
        .route("/{discord_id}/lookups", get(lookups::stream))
        .route("/{discord_id}/timeline", get(timeline::timeline))
        .route("/{discord_id}/keys", get(rail::keys))
        .route("/{discord_id}/links", get(rail::links))
        .route("/{discord_id}/limits", get(rail::limits))
        .route("/{discord_id}/starfish", get(starfish::starfish))
        .merge(mutations::router())
}

pub fn parse_discord_id(raw: &str) -> Result<i64, AdminError> {
    let digits = SNOWFLAKE_DIGITS.contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_digit());
    let parsed = digits.then(|| raw.parse::<i64>().ok()).flatten();
    parsed.ok_or_else(|| AdminError::BadRequest(format!("{raw:?} is not a Discord id")))
}

impl AccountTarget {
    pub async fn resolve(state: &AppState, raw: &str) -> Result<Self, AdminError> {
        let discord_id = parse_discord_id(raw)?;
        let member_id = sqlx::query_scalar("SELECT id FROM members WHERE discord_id = $1")
            .bind(discord_id)
            .fetch_optional(state.db.pool())
            .await
            .context("look up the account's member row")?;
        Ok(Self {
            discord_id,
            member_id,
        })
    }
}

async fn by_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
) -> Result<Json<DiscordIdOf>, AdminError> {
    let member_id: i64 = member_id
        .parse()
        .map_err(|_| AdminError::BadRequest(format!("{member_id:?} is not a member id")))?;
    let discord_id = sqlx::query_scalar("SELECT discord_id FROM members WHERE id = $1")
        .bind(member_id)
        .fetch_optional(state.db.pool())
        .await
        .context("look up the member's Discord id")?
        .ok_or_else(|| AdminError::NotFound(format!("no member #{member_id}")))?;
    Ok(Json(DiscordIdOf { discord_id }))
}

async fn by_starfish(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<DiscordIdOf>, AdminError> {
    let user_id: i64 = user_id
        .parse()
        .map_err(|_| AdminError::BadRequest(format!("{user_id:?} is not a Starfish user id")))?;
    let discord_id = sqlx::query_scalar("SELECT discord_id FROM starfish_users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(state.db.pool())
        .await
        .context("look up the Starfish user's Discord id")?
        .ok_or_else(|| AdminError::NotFound(format!("no Starfish user #{user_id}")))?;
    Ok(Json(DiscordIdOf { discord_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_ids_are_snowflakes_of_seventeen_to_twenty_digits() {
        assert_eq!(
            parse_discord_id("354598337325826048").unwrap(),
            354_598_337_325_826_048
        );
        for raw in [
            "",
            "12345",
            "35459833732582604a",
            "123456789012345678901",
            "-35459833732582604",
        ] {
            assert_eq!(
                parse_discord_id(raw).unwrap_err().code(),
                "bad_request",
                "{raw}"
            );
        }
    }
}
