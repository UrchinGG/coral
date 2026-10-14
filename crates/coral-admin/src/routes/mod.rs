use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

use crate::security::json_error;
use crate::state::AppState;

mod actions;
mod blacklist;
mod guilds;
mod members;
mod overview;
mod players;
mod plugins;
mod requests;
mod resolve;
mod review_guide;
mod review_rules;
mod server;
mod starfish;

const ASSETS_DIR: &str = "assets";
const INDEX_HTML: &str = "index.html";
const PRIVATE_IMMUTABLE: &str = "private, max-age=31536000, immutable";

#[derive(RustEmbed)]
#[folder = "ui/dist/"]
struct Assets;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/members", members::router())
        .nest("/blacklist", blacklist::router())
        .nest("/players", players::router())
        .nest("/guilds", guilds::router())
        .nest("/requests", requests::router())
        .nest("/resolve", resolve::router())
        .nest("/plugins", plugins::router())
        .nest("/starfish", starfish::router())
        .nest("/actions", actions::router())
        .nest("/overview", overview::router())
        .nest("/server", server::router())
        .nest("/review-guide", review_guide::router())
        .nest("/review-rules", review_rules::router())
}

pub fn ui_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/assets/{*path}", get(serve_asset))
        .fallback(get(serve_index))
}

pub async fn api_not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "No such endpoint")
}

async fn serve_asset(Path(path): Path<String>) -> Response {
    let Some(asset) = Assets::get(&format!("{ASSETS_DIR}/{path}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (
        [
            (header::CONTENT_TYPE, asset.metadata.mimetype()),
            (header::CACHE_CONTROL, PRIVATE_IMMUTABLE),
        ],
        Body::from(asset.data),
    )
        .into_response()
}

async fn serve_index() -> Response {
    let Some(index) = Assets::get(INDEX_HTML) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (
        [(header::CONTENT_TYPE, index.metadata.mimetype())],
        Body::from(index.data),
    )
        .into_response()
}
