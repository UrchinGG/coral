use anyhow::Result;
use mongodb::{Database, bson::doc};
use serde::Deserialize;
use tracing::{info, warn};

use crate::sink::{BlacklistRow, Sink, TagRow};

const BATCH_SIZE: usize = 200;

const VALID_TAG_TYPES: &[&str] = &[
    "sniper",
    "blatant_cheater",
    "closet_cheater",
    "confirmed_cheater",
];

#[derive(Debug, Deserialize)]
struct MongoBlacklistPlayer {
    uuid: String,
    is_locked: Option<bool>,
    lock_reason: Option<String>,
    locked_by: Option<mongodb::bson::Bson>,
    lock_timestamp: Option<mongodb::bson::DateTime>,
    tags: Option<Vec<MongoTag>>,
}

#[derive(Debug, Deserialize)]
struct MongoTag {
    tag_type: String,
    reason: Option<String>,
    added_by: Option<mongodb::bson::Bson>,
    added_on: Option<String>,
    hide_username: Option<bool>,
}

fn bson_i64(val: &mongodb::bson::Bson) -> Option<i64> {
    use mongodb::bson::Bson;
    match val {
        Bson::Int64(n) => Some(*n),
        Bson::Int32(n) => Some(*n as i64),
        Bson::Double(n) => Some(*n as i64),
        Bson::Decimal128(d) => d.to_string().split('.').next()?.parse().ok(),
        Bson::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn normalize_timestamp(s: &str) -> String {
    if s.contains('+') || s.ends_with('Z') {
        return s.to_string();
    }
    format!("{s}+00:00")
}

fn map_tag(tag: &MongoTag) -> Option<TagRow> {
    let reason = tag.reason.as_deref().unwrap_or("").to_string();
    let added_by = tag.added_by.as_ref().and_then(bson_i64).unwrap_or(0);
    let added_on = tag.added_on.as_deref().map(normalize_timestamp);
    let hide_username = tag.hide_username.unwrap_or(false);

    if tag.tag_type == "caution" {
        if reason.to_lowercase().contains("replays needed") {
            return Some(TagRow {
                tag_type: "replays_needed".into(),
                reason: String::new(),
                added_by,
                added_on,
                hide_username,
            });
        }
        return None;
    }

    if !VALID_TAG_TYPES.contains(&tag.tag_type.as_str()) {
        return None;
    }

    Some(TagRow {
        tag_type: tag.tag_type.clone(),
        reason,
        added_by,
        added_on,
        hide_username,
    })
}

impl MongoBlacklistPlayer {
    fn to_row(&self) -> Option<BlacklistRow> {
        let tags: Vec<TagRow> = self
            .tags
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(map_tag)
            .collect();

        if tags.is_empty() {
            return None;
        }

        let locked_at = self.lock_timestamp.map(|dt| {
            chrono::DateTime::from_timestamp_millis(dt.timestamp_millis())
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339()
        });

        Some(BlacklistRow {
            uuid: self.uuid.clone(),
            is_locked: self.is_locked.unwrap_or(false),
            lock_reason: self.lock_reason.clone(),
            locked_by: self.locked_by.as_ref().and_then(bson_i64),
            locked_at,
            tags,
        })
    }
}

pub async fn migrate(mongo_db: &Database, sink: &Sink) -> Result<usize> {
    let collection = mongo_db.collection::<MongoBlacklistPlayer>("blacklist");
    let mut cursor = collection.find(doc! {}).await?;

    let mut count = 0;
    let mut skipped = 0;
    let mut errors = 0;
    let mut batch = Vec::with_capacity(BATCH_SIZE);

    while cursor.advance().await? {
        let player = match cursor.deserialize_current() {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to deserialize blacklist player: {e}");
                errors += 1;
                continue;
            }
        };

        match player.to_row() {
            Some(row) => batch.push(row),
            None => {
                skipped += 1;
                continue;
            }
        }

        if batch.len() >= BATCH_SIZE {
            let errs = sink.insert_blacklist(&batch).await;
            count += batch.len() - errs;
            errors += errs;
            batch.clear();
            info!("  {count} players migrated ({errors} errors, {skipped} skipped)");
        }
    }

    if !batch.is_empty() {
        let errs = sink.insert_blacklist(&batch).await;
        count += batch.len() - errs;
        errors += errs;
    }

    if errors > 0 {
        warn!("Blacklist completed with {errors} errors");
    }
    info!("Blacklist: {count} migrated, {skipped} skipped (no valid tags)");
    Ok(count)
}
