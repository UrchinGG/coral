use axum::{Router, response::Html, routing::get};

use crate::state::AppState;

mod blacklist;
mod guilds;
mod members;
mod players;
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
}

pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/", get(serve_ui))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
}

async fn serve_ui() -> ([(&'static str, &'static str); 1], Html<&'static str>) {
    (
        [("cache-control", "no-store")],
        Html(include_str!("../ui/index.html")),
    )
}

async fn serve_css() -> ([(&'static str, &'static str); 2], &'static str) {
    (
        [("content-type", "text/css"), ("cache-control", "no-store")],
        include_str!("../ui/style.css"),
    )
}

async fn serve_js() -> ([(&'static str, &'static str); 2], &'static str) {
    (
        [
            ("content-type", "application/javascript"),
            ("cache-control", "no-store"),
        ],
        include_str!("../ui/app.js"),
    )
}
