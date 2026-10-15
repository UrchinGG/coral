use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use rust_embed::RustEmbed;

use crate::auth::AdminActor;
use crate::boot::{self, BootSource};
use crate::error::AdminError;
use crate::security::json_error;
use crate::state::AppState;

mod accept_encoding;
mod accounts;
mod actions;
mod extensions;
mod flags;
mod ips;
mod keys;
mod labels;
mod maintenance;
mod moderation_recent;
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
const INDEX_HTML: &str = "index.html";
const PRIVATE_IMMUTABLE: &str = "private, max-age=31536000, immutable";
const BROTLI_SUFFIX: &str = ".br";

#[derive(RustEmbed)]
#[folder = "ui/dist/"]
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
        .nest("/requests", request_log::router())
        .nest("/players", players::router())
        .nest("/extensions", extensions::router())
        .merge(platform::router())
        .nest("/accounts", accounts::router())
        .nest("/ips", ips::router())
        .nest("/keys", keys::router())
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
        .fallback(get(serve_index))
        .with_state(boot)
}

pub async fn api_not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "No such endpoint")
}

async fn serve_asset(
    Path(path): Path<String>,
    accept_encoding::AcceptsBrotli(accepts_brotli): accept_encoding::AcceptsBrotli,
) -> Response {
    let name = format!("{ASSETS_DIR}/{path}");
    let Some(asset) = Assets::get(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let brotli = accepts_brotli
        .then(|| Assets::get(&format!("{name}{BROTLI_SUFFIX}")))
        .flatten();
    let content_encoding = brotli
        .is_some()
        .then_some([(header::CONTENT_ENCODING, accept_encoding::BROTLI)]);
    let data = brotli.map_or(asset.data, |encoded| encoded.data);
    (
        [
            (header::CONTENT_TYPE, asset.metadata.mimetype()),
            (header::CACHE_CONTROL, PRIVATE_IMMUTABLE),
            (header::VARY, header::ACCEPT_ENCODING.as_str()),
        ],
        content_encoding,
        Body::from(data),
    )
        .into_response()
}

async fn serve_index(
    State(boot): State<BootSource>,
    Extension(actor): Extension<AdminActor>,
) -> Result<Html<String>, AdminError> {
    let index = Assets::get(INDEX_HTML)
        .ok_or_else(|| AdminError::NotFound("the admin UI is not embedded in this build".into()))?;
    let html = std::str::from_utf8(&index.data).map_err(|error| {
        AdminError::Internal(format!("the embedded index.html is not UTF-8: {error}"))
    })?;
    Ok(Html(boot::inline_into(html, &boot.boot(actor.discord_id))?))
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    const DECODE_BUFFER_BYTES: usize = 4096;

    fn decoded(compressed: &[u8]) -> Vec<u8> {
        let mut plain = Vec::new();
        brotli_decompressor::Decompressor::new(compressed, DECODE_BUFFER_BYTES)
            .read_to_end(&mut plain)
            .unwrap();
        plain
    }

    #[test]
    fn every_precompressed_asset_decodes_to_the_asset_it_stands_for() {
        let plain_names: Vec<String> = Assets::iter()
            .filter_map(|name| name.strip_suffix(BROTLI_SUFFIX).map(str::to_owned))
            .collect();
        assert!(!plain_names.is_empty());
        for name in plain_names {
            let plain = Assets::get(&name).unwrap().data;
            let brotli = Assets::get(&format!("{name}{BROTLI_SUFFIX}")).unwrap().data;
            assert!(
                decoded(&brotli) == plain.as_ref(),
                "{name}{BROTLI_SUFFIX} does not decode to {name}"
            );
        }
    }
}
