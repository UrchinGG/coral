use std::collections::HashMap;

use axum::{Json, Router, extract::*, routing::get};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

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
    let mut out = Resolved::default();

    let uuids = split(p.uuids);
    if !uuids.is_empty() {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT DISTINCT ON (uuid) uuid, username FROM player_snapshots
             WHERE uuid = ANY($1) AND username IS NOT NULL ORDER BY uuid, timestamp DESC",
        )
        .bind(&uuids)
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default();
        out.uuids = rows.into_iter().collect();
    }

    for chunk in split(p.discord).chunks(8) {
        let mut set = tokio::task::JoinSet::new();
        for id in chunk {
            let state = state.clone();
            let id = id.clone();
            set.spawn(async move {
                let name = discord_username(&state, &id).await;
                (id, name)
            });
        }
        while let Some(Ok((id, Some(name)))) = set.join_next().await {
            out.discord.insert(id, name);
        }
    }

    Json(out)
}

async fn discord_username(state: &AppState, id: &str) -> Option<String> {
    let cache_key = format!("cache:discord:{id}");
    let mut conn = state.redis.as_ref()?.connection();
    if let Ok(name) = conn.get::<_, String>(&cache_key).await {
        return Some(name);
    }

    let token = state.discord_token.as_ref()?;
    let user = state
        .http
        .get(format!("https://discord.com/api/v10/users/{id}"))
        .header("Authorization", format!("Bot {token}"))
        .send()
        .await
        .ok()?
        .json::<DiscordUser>()
        .await
        .ok()?;
    let _: Result<(), _> = conn.set_ex(&cache_key, &user.username, 900).await;
    Some(user.username)
}

#[derive(Deserialize)]
struct DiscordUser {
    username: String,
}
