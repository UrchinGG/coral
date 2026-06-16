use anyhow::Result;
use mongodb::{Database, bson::doc};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::sink::{MemberRow, Sink};

const BATCH_SIZE: usize = 200;

#[derive(Debug, Deserialize)]
struct MongoMember {
    discord_id: serde_json::Value,
    uuid: Option<String>,
    join_date: Option<String>,
    request_count: Option<i64>,
    config: Option<serde_json::Value>,
    key_locked: Option<bool>,
    minecraft_accounts: Option<Vec<String>>,
}

impl MongoMember {
    fn discord_id_i64(&self) -> Option<i64> {
        match &self.discord_id {
            serde_json::Value::Number(n) => n.as_i64(),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    fn to_row(&self) -> Option<MemberRow> {
        Some(MemberRow {
            discord_id: self.discord_id_i64()?,
            uuid: self.uuid.clone(),
            join_date: self.join_date.clone(),
            request_count: self.request_count.unwrap_or(0),
            tagging_disabled: self.key_locked.unwrap_or(false),
            config: self.config.clone().unwrap_or_else(|| json!({})),
            minecraft_accounts: self.minecraft_accounts.clone().unwrap_or_default(),
        })
    }
}

pub async fn migrate(mongo_db: &Database, sink: &Sink) -> Result<usize> {
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

        match member.to_row() {
            Some(row) => batch.push(row),
            None => {
                warn!(
                    "Skipping member with invalid discord_id: {:?}",
                    member.discord_id
                );
                errors += 1;
                continue;
            }
        }

        if batch.len() >= BATCH_SIZE {
            let errs = sink.insert_members(&batch).await;
            count += batch.len() - errs;
            errors += errs;
            batch.clear();
            info!("  {count} members migrated ({errors} errors)");
        }
    }

    if !batch.is_empty() {
        let errs = sink.insert_members(&batch).await;
        count += batch.len() - errs;
        errors += errs;
    }

    if errors > 0 {
        warn!("Members completed with {errors} errors");
    }
    Ok(count)
}
