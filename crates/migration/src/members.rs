use anyhow::Result;
use mongodb::{Database, bson::doc};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::client::CoralClient;

const BATCH_SIZE: usize = 200;

#[derive(Debug, Deserialize)]
struct MongoMember {
    discord_id: serde_json::Value,
    uuid: Option<String>,
    api_key: Option<String>,
    join_date: Option<String>,
    request_count: Option<i64>,
    config: Option<serde_json::Value>,
    is_admin: Option<bool>,
    is_mod: Option<bool>,
    private: Option<bool>,
    key_locked: Option<bool>,
    ip_history: Option<Vec<IpHistoryEntry>>,
    minecraft_accounts: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct IpHistoryEntry {
    ip_address: Option<String>,
    first_seen: Option<String>,
}

impl MongoMember {
    fn discord_id_i64(&self) -> Option<i64> {
        match &self.discord_id {
            serde_json::Value::Number(n) => n.as_i64(),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    fn access_level(&self) -> i16 {
        if self.is_admin.unwrap_or(false) {
            4
        } else if self.is_mod.unwrap_or(false) {
            3
        } else if self.private.unwrap_or(false) {
            1
        } else {
            0
        }
    }

    fn to_payload(&self) -> Option<serde_json::Value> {
        let discord_id = self.discord_id_i64()?;

        let ip_history: Vec<serde_json::Value> = self.ip_history.as_deref().unwrap_or(&[])
            .iter()
            .filter_map(|ip| {
                Some(json!({
                    "ip_address": ip.ip_address.as_ref()?,
                    "first_seen": ip.first_seen,
                }))
            })
            .collect();

        Some(json!({
            "discord_id": discord_id,
            "uuid": self.uuid,
            "api_key": self.api_key,
            "join_date": self.join_date,
            "request_count": self.request_count.unwrap_or(0),
            "access_level": self.access_level(),
            "key_locked": self.key_locked.unwrap_or(false),
            "config": self.config.clone().unwrap_or_else(|| json!({})),
            "ip_history": ip_history,
            "minecraft_accounts": self.minecraft_accounts.clone().unwrap_or_default(),
        }))
    }
}

pub async fn migrate(mongo_db: &Database, client: &CoralClient) -> Result<usize> {
    let collection = mongo_db.collection::<MongoMember>("members");
    let mut cursor = collection.find(doc! {}).await?;

    let mut count = 0;
    let mut errors = 0;
    let mut batch = Vec::with_capacity(BATCH_SIZE);

    while cursor.advance().await? {
        let member = match cursor.deserialize_current() {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to deserialize member: {e}");
                errors += 1;
                continue;
            }
        };

        match member.to_payload() {
            Some(payload) => batch.push(payload),
            None => {
                warn!("Skipping member with invalid discord_id: {:?}", member.discord_id);
                errors += 1;
                continue;
            }
        }

        if batch.len() >= BATCH_SIZE {
            let result = client.post(&json!({"type": "members", "data": batch})).await?;
            let errs = result["errors"].as_u64().unwrap_or(0);
            count += batch.len() - errs as usize;
            errors += errs as usize;
            batch.clear();
            info!("  {count} members migrated ({errors} errors)");
        }
    }

    if !batch.is_empty() {
        let result = client.post(&json!({"type": "members", "data": batch})).await?;
        let errs = result["errors"].as_u64().unwrap_or(0);
        count += batch.len() - errs as usize;
        errors += errs as usize;
    }

    if errors > 0 {
        warn!("Members completed with {errors} errors");
    }
    Ok(count)
}
