use std::collections::HashMap;

use chrono::{Duration, Utc};
use clients::is_uuid;
use database::{CacheRepository, DiscordUsernameCacheRepository};
use serde::Deserialize;

use crate::state::AppState;

const STALE_AFTER: Duration = Duration::hours(24);
const DISCORD_FETCH_CONCURRENCY: usize = 8;

#[derive(Default)]
pub struct Identities {
    pub discord: HashMap<i64, String>,
    pub minecraft: HashMap<String, String>,
}

pub async fn resolve(state: &AppState, discord_ids: &[i64], uuids: &[String]) -> Identities {
    let (discord, minecraft) = tokio::join!(
        resolve_discord_usernames(state, discord_ids),
        resolve_minecraft_usernames(state, uuids),
    );
    Identities { discord, minecraft }
}

pub async fn resolve_discord_usernames(state: &AppState, ids: &[i64]) -> HashMap<i64, String> {
    let ids: Vec<i64> = ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return HashMap::new();
    }

    let cache = DiscordUsernameCacheRepository::new(state.db.pool());
    let cached = cache.get_many(&ids).await.unwrap_or_default();

    let mut out = HashMap::new();
    let mut stale = Vec::new();
    let mut missing = Vec::new();
    for id in &ids {
        match cached.get(id) {
            Some(entry) => {
                out.insert(*id, entry.username.clone());
                if Utc::now() - entry.last_refreshed > STALE_AFTER {
                    stale.push(*id);
                }
            }
            None => missing.push(*id),
        }
    }

    if !missing.is_empty() {
        for (id, username) in fetch_discord_usernames(state, &missing).await {
            out.insert(id, username);
        }
    }
    if !stale.is_empty() {
        let state = state.clone();
        tokio::spawn(async move {
            fetch_discord_usernames(&state, &stale).await;
        });
    }
    out
}

async fn fetch_discord_usernames(state: &AppState, ids: &[i64]) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    let cache = DiscordUsernameCacheRepository::new(state.db.pool());
    for chunk in ids.chunks(DISCORD_FETCH_CONCURRENCY) {
        let mut set = tokio::task::JoinSet::new();
        for &id in chunk {
            let state = state.clone();
            set.spawn(async move { (id, fetch_discord_username(&state, id).await) });
        }
        while let Some(Ok((id, username))) = set.join_next().await {
            if let Some(username) = username {
                cache.upsert(id, &username).await.ok();
                out.insert(id, username);
            }
        }
    }
    out
}

#[derive(Deserialize)]
struct DiscordUser {
    username: String,
}

async fn fetch_discord_username(state: &AppState, discord_id: i64) -> Option<String> {
    let token = state.discord_token.as_ref()?;
    let user = state
        .http
        .get(format!("https://discord.com/api/v10/users/{discord_id}"))
        .header("Authorization", format!("Bot {token}"))
        .send()
        .await
        .ok()?
        .json::<DiscordUser>()
        .await
        .ok()?;
    Some(user.username)
}

pub async fn resolve_minecraft_usernames(
    state: &AppState,
    uuids: &[String],
) -> HashMap<String, String> {
    let uuids: Vec<String> = uuids
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if uuids.is_empty() {
        return HashMap::new();
    }

    let cache = CacheRepository::new(state.db.pool());
    let mut out = cache.usernames(&uuids).await.unwrap_or_default();

    let missing: Vec<String> = uuids
        .iter()
        .filter(|u| !out.contains_key(*u))
        .cloned()
        .collect();
    if !missing.is_empty() {
        let mut set = tokio::task::JoinSet::new();
        for uuid in missing {
            let mojang = state.mojang.clone();
            set.spawn(async move {
                let username = mojang.get_profile(&uuid).await.ok().map(|p| p.username);
                (uuid, username)
            });
        }
        while let Some(Ok((uuid, username))) = set.join_next().await {
            if let Some(username) = username {
                out.insert(uuid, username);
            }
        }
    }
    out
}

pub async fn resolve_minecraft_uuid(state: &AppState, query: &str) -> Option<String> {
    if is_uuid(query) {
        return Some(clients::normalize_uuid(query));
    }
    state.mojang.resolve(query).await.ok().map(|p| p.uuid)
}

pub async fn member_ids_by_discord_id(state: &AppState, discord_ids: &[i64]) -> HashMap<i64, i64> {
    if discord_ids.is_empty() {
        return HashMap::new();
    }
    sqlx::query_as::<_, (i64, i64)>("SELECT discord_id, id FROM members WHERE discord_id = ANY($1)")
        .bind(discord_ids)
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default()
        .into_iter()
        .collect()
}

pub fn looks_like_ign(s: &str) -> bool {
    !s.is_empty() && s.len() <= 16 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_ign_accepts_valid_usernames() {
        assert!(looks_like_ign("Notch"));
        assert!(looks_like_ign("player_123"));
        assert!(!looks_like_ign(""));
        assert!(!looks_like_ign("this_name_is_too_long_to_be_valid"));
        assert!(!looks_like_ign("has space"));
    }
}
