use axum::{Router, response::Html, routing::get};

use crate::state::AppState;

mod blacklist;
mod diagnostics;
mod members;
mod rate_limits;
mod snapshots;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/members", members::router())
        .nest("/blacklist", blacklist::router())
        .nest("/snapshots", snapshots::router())
        .nest("/rate-limits", rate_limits::router())
        .nest("/diagnostics", diagnostics::router())
}

pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/", get(serve_ui))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
}

async fn serve_ui() -> Html<&'static str> {
    Html(include_str!("../ui/index.html"))
}

async fn serve_css() -> ([(&'static str, &'static str); 1], &'static str) {
    (
        [("content-type", "text/css")],
        include_str!("../ui/style.css"),
    )
}

async fn serve_js() -> ([(&'static str, &'static str); 1], &'static str) {
    (
        [("content-type", "application/javascript")],
        include_str!("../ui/app.js"),
    )
}
