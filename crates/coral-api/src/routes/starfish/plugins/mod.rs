mod actions;
mod browse;
mod dto;
mod github_api;
mod management;
mod manifest;
mod publish;

use axum::{Router, middleware, routing::{get, post}};

use crate::state::AppState;

use super::session_auth::require_starfish_session;


pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/plugins", get(browse::list_plugins))
        .route("/plugins/disabled", get(browse::list_disabled))
        .route("/plugins/installed", get(actions::list_installed))
        .route("/plugins/mine", get(management::list_mine))
        .route("/plugins/publish", post(publish::publish_plugin))
        .route("/plugins/{slug}",
            get(browse::get_plugin)
            .patch(management::patch_plugin)
            .delete(management::delete_plugin))
        .route("/plugins/{slug}/body", get(browse::download_body))
        .route("/plugins/{slug}/install", post(actions::install_plugin).delete(actions::uninstall_plugin))
        .route("/plugins/{slug}/rate", post(actions::rate_plugin))
        .route("/plugins/{slug}/unlist", post(management::set_unlisted))
        .route("/plugins/{slug}/official", post(management::set_official))
        .route("/plugins/{slug}/releases/{version}",
            axum::routing::delete(management::delete_release))
        .route("/plugins/{slug}/releases/{version}/yank", post(management::yank_release))
        .route("/plugins/{slug}/releases/{version}/unyank", post(management::unyank_release))
        .route_layer(middleware::from_fn_with_state(state, require_starfish_session))
}
