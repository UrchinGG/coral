use axum::Router;
use axum::routing::{get, post, put};

use crate::error::AdminError;
use crate::state::AppState;

mod guilds;
mod moderation;
mod page;
mod plugins;
mod review;
mod server;
mod starfish;
mod tools;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/moderation", moderation_router())
        .nest("/server", server_router())
        .nest("/starfish", starfish_router())
        .nest("/plugins", plugins_router())
        .nest("/guilds", guilds_router())
        .route("/tools/resolve", post(tools::resolve))
}

fn moderation_router() -> Router<AppState> {
    Router::new()
        .route("/tag-types", get(moderation::tag_types))
        .route("/blacklist", get(moderation::blacklist))
        .route("/strikes", get(moderation::strikes))
        .route("/audit", get(moderation::audit))
        .route("/guide", get(review::guide).put(review::save_guide))
        .route("/guide/ping-roles", put(review::set_ping_roles))
        .route("/guide/publish", post(review::publish_guide))
        .route("/rules", get(review::rules).put(review::save_rules))
}

fn server_router() -> Router<AppState> {
    Router::new()
        .route("/", get(server::config))
        .route("/directory", get(server::directory::directory))
        .route("/population", get(server::directory::population))
        .route("/link-role", put(server::set_link_role))
        .route("/unlinked-role", put(server::set_unlinked_role))
        .route("/link-channel", put(server::set_link_channel))
        .route("/nickname-template", put(server::set_nickname_template))
        .route("/nicknames/reset", post(server::reset_nicknames))
        .route("/rules", post(server::add_rule))
        .route(
            "/rules/{rule_id}",
            put(server::update_rule).delete(server::remove_rule),
        )
        .route("/roles/{role_id}/strip", post(server::strip_role))
        .route("/jobs", get(server::jobs::list))
        .route("/jobs/{job_id}/cancel", post(server::jobs::cancel))
        .route("/jobs/{job_id}/events", get(server::jobs::events))
}

fn starfish_router() -> Router<AppState> {
    Router::new().route("/users", get(starfish::users))
}

fn plugins_router() -> Router<AppState> {
    Router::new()
        .route("/", get(plugins::list))
        .route(
            "/{slug}",
            get(plugins::detail).delete(plugins::delete_plugin),
        )
        .route("/{slug}/official", post(plugins::set_official))
        .route("/{slug}/unlisted", post(plugins::set_unlisted))
        .route("/{slug}/disabled", post(plugins::set_disabled))
        .route(
            "/{slug}/releases/{version}",
            axum::routing::delete(plugins::delete_release),
        )
        .route(
            "/{slug}/releases/{version}/yank",
            post(plugins::yank_release),
        )
        .route(
            "/{slug}/releases/{version}/unyank",
            post(plugins::unyank_release),
        )
        .route(
            "/{slug}/reviews/{user_id}",
            axum::routing::delete(plugins::delete_review),
        )
}

fn guilds_router() -> Router<AppState> {
    Router::new()
        .route("/", get(guilds::list))
        .route("/{guild_id}", get(guilds::detail))
        .route("/{guild_id}/at", get(guilds::at))
}

fn parse_role(raw: Option<&str>) -> Result<Option<i64>, AdminError> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .map(Some)
        .ok_or_else(|| AdminError::BadRequest(format!("{raw:?} is not a Discord id")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_ids_mean_none_and_garbage_is_rejected() {
        assert_eq!(parse_role(None).unwrap(), None);
        assert_eq!(parse_role(Some("  ")).unwrap(), None);
        assert_eq!(
            parse_role(Some("1200000000000000001")).unwrap(),
            Some(1_200_000_000_000_000_001)
        );
        assert_eq!(parse_role(Some("@here")).unwrap_err().code(), "bad_request");
        assert_eq!(parse_role(Some("-4")).unwrap_err().code(), "bad_request");
    }
}
