mod actions;
mod attestation;
mod browse;
mod content_hash;
mod dto;
mod github_api;
mod management;
mod manifest;
mod publish;

use axum::{
    Router, middleware,
    routing::{get, post},
};

use database::PluginRegistryRepository;

use crate::state::AppState;

use super::session_auth::require_starfish_session;

pub async fn backfill_release_content_hashes(db: std::sync::Arc<database::Database>) {
    let repo = PluginRegistryRepository::new(db.pool());
    let releases = match repo.list_releases_missing_content_hash().await {
        Ok(releases) => releases,
        Err(e) => {
            tracing::warn!("content hash backfill query failed: {e}");
            return;
        }
    };

    for (release_id, zip_bytes) in releases {
        match content_hash::compute_content_sha256(&zip_bytes) {
            Ok(hash) => match repo.set_release_content_hash(release_id, &hash).await {
                Ok(()) => tracing::info!("backfilled content hash for release {release_id}"),
                Err(e) => {
                    tracing::warn!("content hash backfill failed for release {release_id}: {e}")
                }
            },
            Err(e) => tracing::warn!("content hash compute failed for release {release_id}: {e:?}"),
        }
    }
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/plugins", get(browse::list_plugins))
        .route("/plugins/disabled", get(browse::list_disabled))
        .route("/plugins/installed", get(actions::list_installed))
        .route(
            "/plugins/attestation-info",
            post(attestation::attestation_info),
        )
        .route("/plugins/mine", get(management::list_mine))
        .route("/plugins/publish", post(publish::publish_plugin))
        .route(
            "/plugins/{slug}",
            get(browse::get_plugin)
                .patch(management::patch_plugin)
                .delete(management::delete_plugin),
        )
        .route("/plugins/{slug}/body", get(browse::download_body))
        .route(
            "/plugins/{slug}/install",
            post(actions::install_plugin).delete(actions::uninstall_plugin),
        )
        .route("/plugins/{slug}/rate", post(actions::rate_plugin))
        .route("/plugins/{slug}/official", post(management::set_official))
        .route("/plugins/{slug}/disable", post(management::set_disabled))
        .route(
            "/plugins/{slug}/releases/{version}",
            axum::routing::delete(management::delete_release),
        )
        .route(
            "/plugins/{slug}/releases/{version}/yank",
            post(management::yank_release),
        )
        .route(
            "/plugins/{slug}/releases/{version}/unyank",
            post(management::unyank_release),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            require_starfish_session,
        ))
}
