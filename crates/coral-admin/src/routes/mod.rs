use axum::body::Body;
use axum::http::{Uri, header};
use axum::response::{IntoResponse, Response};
use axum::{Router, http::StatusCode};
use rust_embed::RustEmbed;

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

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/members", members::router())
        .nest("/blacklist", blacklist::router())
        .nest("/players", players::router())
        .nest("/guilds", guilds::router())
        .nest("/requests", requests::router())
        .nest("/resolve", resolve::router())
        .nest("/plugins", plugins::router())
        .nest("/actions", actions::router())
        .nest("/overview", overview::router())
}

#[derive(RustEmbed)]
#[folder = "ui/dist/"]
struct Assets;

pub fn ui_router() -> Router<AppState> {
    Router::new().fallback(serve_asset)
}

async fn serve_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    match Assets::get(path) {
        Some(asset) => asset_response(path, asset),
        None => match Assets::get("index.html") {
            Some(asset) => asset_response("index.html", asset),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

fn asset_response(path: &str, asset: rust_embed::EmbeddedFile) -> Response {
    let mime = asset.metadata.mimetype();
    let cache_control = if path == "index.html" {
        "no-store"
    } else {
        "public, max-age=31536000, immutable"
    };
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CACHE_CONTROL, cache_control.to_string()),
        ],
        Body::from(asset.data),
    )
        .into_response()
}
