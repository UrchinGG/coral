use anyhow::Result;
use chrono::{DateTime, Utc};
use database::calculate_delta;
use mongodb::{Database, bson::doc};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tracing::{info, warn};

use crate::client::CoralClient;

const PLAYER_BATCH_SIZE: usize = 2000;
const SNAPSHOT_BATCH_SIZE: usize = 5000;

#[derive(Debug, Deserialize)]
struct MongoCacheDocument {
    uuid: String,
    data: Vec<Value>,
}

struct PendingSnapshot {
    uuid: String,
    timestamp: DateTime<Utc>,
    username: String,
    is_baseline: bool,
    data: Value,
}

impl PendingSnapshot {
    fn to_payload(&self) -> Value {
        json!({
            "uuid": self.uuid,
            "timestamp": self.timestamp.to_rfc3339(),
            "username": self.username,
            "is_baseline": self.is_baseline,
            "data": self.data,
        })
    }
}

pub async fn migrate(mongo_db: &Database, client: &CoralClient) -> Result<usize> {
    let collection = mongo_db.collection::<MongoCacheDocument>("cache");
    let mut cursor = collection.find(doc! {}).await?;

    let mut players = 0;
    let mut snapshots = 0;
    let mut errors = 0;
    let mut batch: Vec<PendingSnapshot> = Vec::new();

    while cursor.advance().await? {
        let doc = match cursor.deserialize_current() {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to deserialize cache document: {e}");
                errors += 1;
                continue;
            }
        };

        collect_player_snapshots(&doc, &mut batch);
        players += 1;

        if players % PLAYER_BATCH_SIZE == 0 {
            let count = flush_batch(client, &batch).await?;
            snapshots += count;
            batch.clear();
            info!("  {players} players ({snapshots} snapshots)...");
        }
    }

    if !batch.is_empty() {
        let count = flush_batch(client, &batch).await?;
        snapshots += count;
    }

    if errors > 0 {
        warn!("Cache completed with {errors} errors");
    }
    info!("Cache: {snapshots} snapshots across {players} players");
    Ok(players)
}

async fn flush_batch(client: &CoralClient, batch: &[PendingSnapshot]) -> Result<usize> {
    if batch.is_empty() {
        return Ok(0);
    }

    let mut total = 0;
    for chunk in batch.chunks(SNAPSHOT_BATCH_SIZE) {
        let payloads: Vec<Value> = chunk.iter().map(|s| s.to_payload()).collect();
        let result = client.post(&json!({"type": "snapshots", "data": payloads})).await?;
        total += result["migrated"].as_u64().unwrap_or(0) as usize;
    }

    Ok(total)
}

fn collect_player_snapshots(doc: &MongoCacheDocument, batch: &mut Vec<PendingSnapshot>) {
    let snapshots = all_snapshots(&doc.data);
    let mut previous: Option<Value> = None;

    for (timestamp, snapshot) in snapshots {
        let transformed = reverse_transform(snapshot);
        let username = snapshot
            .get("display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let is_baseline = previous.is_none();
        let data = match &previous {
            None => transformed.clone(),
            Some(prev) => match calculate_delta(prev, &transformed) {
                Some(delta) => delta,
                None => continue,
            },
        };

        batch.push(PendingSnapshot {
            uuid: doc.uuid.clone(),
            timestamp,
            username,
            is_baseline,
            data,
        });

        previous = Some(transformed);
    }
}

fn all_snapshots(data: &[Value]) -> Vec<(DateTime<Utc>, &Value)> {
    let mut snapshots: Vec<(DateTime<Utc>, &Value)> = data
        .iter()
        .filter_map(|entry| {
            let player = entry.get("player")?;
            if player.is_null() {
                return None;
            }
            let ts = entry.get("timestamp")?.as_f64()?;
            let datetime = DateTime::from_timestamp(ts as i64, 0)?;
            Some((datetime, player))
        })
        .collect();

    snapshots.sort_by_key(|(ts, _)| *ts);
    snapshots
}

fn reverse_transform(urchin: &Value) -> Value {
    let mut player = Map::new();

    if let Some(v) = urchin.get("display_name") {
        player.insert("displayname".into(), v.clone());
    }
    if let Some(v) = urchin.get("rank") {
        player.insert("prefix".into(), v.clone());
    }
    if let Some(v) = urchin.get("network_exp") {
        player.insert("networkExp".into(), v.clone());
    }
    if let Some(v) = urchin.get("karma") {
        player.insert("karma".into(), v.clone());
    }
    if let Some(v) = urchin.get("achievement_points") {
        player.insert("achievementPoints".into(), v.clone());
    }
    if let Some(v) = urchin.get("first_login") {
        player.insert("firstLogin".into(), v.clone());
    }
    if let Some(v) = urchin.get("last_login") {
        player.insert("lastLogin".into(), v.clone());
    }
    if let Some(v) = urchin.get("last_logout") {
        player.insert("lastLogout".into(), v.clone());
    }
    if let Some(v) = urchin.get("ranks_gifted") {
        player.insert("giftingMeta".into(), json!({ "ranksGiven": v }));
    }

    if let Some(bw) = urchin.get("bedwars") {
        let mut raw_bw = Map::new();

        if let Some(v) = bw.get("experience") {
            raw_bw.insert("Experience".into(), v.clone());
        }

        if let Some(modes) = bw.get("modes") {
            reverse_mode(&mut raw_bw, modes, "overall", "");
            reverse_mode(&mut raw_bw, modes, "solos", "eight_one_");
            reverse_mode(&mut raw_bw, modes, "doubles", "eight_two_");
            reverse_mode(&mut raw_bw, modes, "threes", "four_three_");
            reverse_mode(&mut raw_bw, modes, "fours", "four_four_");
            reverse_mode(&mut raw_bw, modes, "fourvfour", "two_four_");
        }

        player.insert("stats".into(), json!({ "Bedwars": raw_bw }));
    }

    Value::Object(player)
}

fn reverse_mode(raw_bw: &mut Map<String, Value>, modes: &Value, mode_name: &str, prefix: &str) {
    let Some(mode) = modes.get(mode_name) else {
        return;
    };

    let set = |map: &mut Map<String, Value>, suffix: &str, value: &Value| {
        map.insert(format!("{prefix}{suffix}"), value.clone());
    };

    if let Some(v) = mode.get("games_played") { set(raw_bw, "games_played_bedwars", v); }
    if let Some(v) = mode.get("wins") { set(raw_bw, "wins_bedwars", v); }
    if let Some(v) = mode.get("losses") { set(raw_bw, "losses_bedwars", v); }
    if let Some(v) = mode.get("final_kills") { set(raw_bw, "final_kills_bedwars", v); }
    if let Some(v) = mode.get("final_deaths") { set(raw_bw, "final_deaths_bedwars", v); }
    if let Some(v) = mode.get("beds_broken") { set(raw_bw, "beds_broken_bedwars", v); }
    if let Some(v) = mode.get("beds_lost") { set(raw_bw, "beds_lost_bedwars", v); }

    if let Some(v) = mode.get("winstreak") {
        let key = if prefix.is_empty() { "winstreak".into() } else { format!("{prefix}winstreak") };
        if v.as_str() == Some("?") {
            raw_bw.insert(key, Value::Null);
        } else {
            raw_bw.insert(key, v.clone());
        }
    }

    if let Some(kills) = mode.get("kills") {
        if let Some(v) = kills.get("kills") { set(raw_bw, "kills_bedwars", v); }
        if let Some(v) = kills.get("projectile_kills") { set(raw_bw, "projectile_kills_bedwars", v); }
        if let Some(v) = kills.get("void_kills") { set(raw_bw, "void_kills_bedwars", v); }
        if let Some(v) = kills.get("fall_kills") { set(raw_bw, "fall_kills_bedwars", v); }
        if let Some(v) = kills.get("explosion_kills") { set(raw_bw, "entity_explosion_kills_bedwars", v); }
        if let Some(v) = kills.get("magic_kills") { set(raw_bw, "magic_kills_bedwars", v); }
        if let Some(v) = kills.get("fire_tick_kills") { set(raw_bw, "fire_tick_kills_bedwars", v); }
    }

    if let Some(deaths) = mode.get("deaths") {
        if let Some(v) = deaths.get("deaths") { set(raw_bw, "deaths_bedwars", v); }
        if let Some(v) = deaths.get("void_deaths") { set(raw_bw, "void_deaths_bedwars", v); }
        if let Some(v) = deaths.get("fall_deaths") { set(raw_bw, "fall_deaths_bedwars", v); }
        if let Some(v) = deaths.get("explosion_deaths") { set(raw_bw, "entity_explosion_deaths_bedwars", v); }
        if let Some(v) = deaths.get("magic_deaths") { set(raw_bw, "magic_deaths_bedwars", v); }
        if let Some(v) = deaths.get("fire_tick_deaths") { set(raw_bw, "fire_tick_deaths_bedwars", v); }
        if let Some(v) = deaths.get("projectile_deaths") { set(raw_bw, "projectile_deaths_bedwars", v); }
    }

    if let Some(resources) = mode.get("resources") {
        if let Some(v) = resources.get("emeralds_collected") { set(raw_bw, "emerald_resources_collected_bedwars", v); }
        if let Some(v) = resources.get("gold_collected") { set(raw_bw, "gold_resources_collected_bedwars", v); }
        if let Some(v) = resources.get("diamonds_collected") { set(raw_bw, "diamond_resources_collected_bedwars", v); }
        if let Some(v) = resources.get("iron_collected") { set(raw_bw, "iron_resources_collected_bedwars", v); }
    }
}
