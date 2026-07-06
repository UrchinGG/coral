use anyhow::Result;
use mongodb::{Database, bson::doc};
use serde::Deserialize;
use tracing::{info, warn};

use crate::blacklist::bson_i64;
use crate::sink::Sink;

const HYPIXEL_API_BASE: &str = "https://api.hypixel.net/v2";

#[derive(Debug, Deserialize)]
struct MongoGuildConfig {
    guild_name: String,
    guild_id: Option<String>,
    #[serde(default)]
    ping_user_ids: Vec<mongodb::bson::Bson>,
}

async fn lookup_guild_id_live(http: &reqwest::Client, api_key: &str, name: &str) -> Option<String> {
    let url = format!("{HYPIXEL_API_BASE}/guild?name={}", urlencoding_lite(name));
    let resp = match http.get(&url).header("API-Key", api_key).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Hypixel lookup failed for guild '{name}': {e}");
            return None;
        }
    };
    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => {
            warn!("Hypixel lookup for guild '{name}' returned unparseable JSON: {e}");
            return None;
        }
    };
    if json["success"].as_bool() != Some(true) {
        warn!("Hypixel lookup for guild '{name}' was not successful: {json}");
        return None;
    }
    hypixel::Guild::from_value(&json["guild"]).map(|g| g.id)
}

fn urlencoding_lite(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            c if c.is_ascii_alphanumeric() || "-_.~".contains(c) => c.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

pub async fn migrate(mongo_db: &Database, sink: &Sink, hypixel_api_key: &str) -> Result<usize> {
    sink.wipe_guild_subscriptions().await?;

    let collection = mongo_db.collection::<MongoGuildConfig>("guild_configs");
    let mut cursor = collection.find(doc! {}).await?;
    let http = reqwest::Client::new();

    let mut rows = Vec::new();
    let mut unresolved = 0;

    while cursor.advance().await? {
        let config = match cursor.deserialize_current() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to deserialize guild_configs doc: {e}");
                continue;
            }
        };

        let guild_id = match &config.guild_id {
            Some(id) => Some(id.clone()),
            None => match sink.resolve_guild_id_by_name(&config.guild_name).await {
                Ok(Some(id)) => {
                    info!(
                        "Resolved '{}' -> {id} via coral's guild_current cache",
                        config.guild_name
                    );
                    Some(id)
                }
                _ => {
                    info!("Looking up '{}' live against Hypixel...", config.guild_name);
                    lookup_guild_id_live(&http, hypixel_api_key, &config.guild_name).await
                }
            },
        };

        let Some(guild_id) = guild_id else {
            warn!(
                "Could not resolve guild_id for '{}', skipping",
                config.guild_name
            );
            unresolved += 1;
            continue;
        };

        for uid in &config.ping_user_ids {
            if let Some(discord_id) = bson_i64(uid) {
                rows.push((guild_id.clone(), discord_id));
            }
        }
    }

    if unresolved > 0 {
        warn!("{unresolved} guild_configs could not be resolved to a guild_id and were skipped");
    }

    let errors = sink.insert_guild_subscriptions(&rows).await;
    if errors > 0 {
        warn!("Guild subscriptions completed with {errors} errors");
    }
    Ok(rows.len() - errors)
}
