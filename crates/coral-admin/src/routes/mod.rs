use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use rust_embed::RustEmbed;

use crate::auth::AdminActor;
use crate::boot::{self, BootSource};
use crate::error::AdminError;
use crate::perimeter::MOUNT;
use crate::security::json_error;
use crate::state::AppState;

mod accounts;
mod actions;
mod extensions;
mod flags;
mod labels;
mod maintenance;
mod moderation_recent;
mod overview;
mod platform;
mod players;
mod ref_labels;
mod request_log;
mod search;
mod session;
mod streams;
mod top_players;
mod traffic;

const ASSETS_DIR: &str = "assets";
const HTML_SUFFIX: &str = ".html";
const HOME: &str = "/account";
const PRIVATE_IMMUTABLE: &str = "private, max-age=31536000, immutable";

#[derive(RustEmbed)]
#[folder = "../../web/sites/admin/dist/"]
struct Assets;

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/session", get(session::boot))
        .nest("/search", search::router())
        .route("/labels", post(labels::labels))
        .route("/maintenance", get(maintenance::overview))
        .route("/maintenance/recompute", post(maintenance::recompute))
        .route("/maintenance/{job}", post(maintenance::start))
        .route("/players/top", get(top_players::top_players))
        .route("/overview", get(overview::overview))
        .nest("/requests", request_log::router())
        .nest("/players", players::router())
        .nest("/extensions", extensions::router())
        .merge(platform::router())
        .nest("/accounts", accounts::router())
        .nest("/traffic", traffic::router())
        .nest("/flags", flags::router())
        .nest("/actions", actions::router())
        .route(
            "/moderation/recent",
            get(moderation_recent::recent_moderation),
        )
        .with_state(state)
}

pub fn ui_router<S>(boot: BootSource) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/assets/{*path}", get(serve_asset))
        .fallback(get(serve_page))
        .with_state(boot)
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

async fn serve_page(
    State(boot): State<BootSource>,
    Extension(actor): Extension<AdminActor>,
    uri: Uri,
) -> Result<Response, AdminError> {
    let path = uri.path().trim_matches('/');
    if path.is_empty() {
        return Ok(Redirect::to(&format!("{MOUNT}{HOME}")).into_response());
    }
    if let Some(page) = Assets::get(&format!("{path}{HTML_SUFFIX}")) {
        let html = std::str::from_utf8(&page.data).map_err(|error| {
            AdminError::Internal(format!("the embedded {path} page is not UTF-8: {error}"))
        })?;
        return Ok(Html(boot::inline_into(html, &boot.boot(actor))?).into_response());
    }
    let file = Assets::get(path)
        .ok_or_else(|| AdminError::NotFound(format!("the admin has no page at /{path}")))?;
    Ok((
        [(header::CONTENT_TYPE, file.metadata.mimetype())],
        Body::from(file.data),
    )
        .into_response())
}
