use std::collections::HashMap;

use axum::{Json, Router, extract::*, routing::get};
use serde::{Deserialize, Serialize};

use crate::identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(resolve))
}

#[derive(Deserialize)]
struct Params {
    uuids: Option<String>,
    discord: Option<String>,
}

#[derive(Serialize, Default)]
struct Resolved {
    uuids: HashMap<String, String>,
    discord: HashMap<String, String>,
}

fn split(s: Option<String>) -> Vec<String> {
    s.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

async fn resolve(State(state): State<AppState>, Query(p): Query<Params>) -> Json<Resolved> {
    let uuids = split(p.uuids);
    let discord_ids: Vec<i64> = split(p.discord)
        .into_iter()
        .filter_map(|id| id.parse().ok())
        .collect();

    let names = identity::resolve(&state, &discord_ids, &uuids).await;
    Json(Resolved {
        uuids: names.minecraft,
        discord: names
            .discord
            .into_iter()
            .map(|(id, name)| (id.to_string(), name))
            .collect(),
    })
}
