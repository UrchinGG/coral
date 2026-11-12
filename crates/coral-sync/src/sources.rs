//! What Coral knows about a member, by source. Each source fills its own namespace
//! of the context the engine reads, except Hypixel, which still sits at the root
//! until the one-time template rewrite moves it under `hypixel.*`.

use std::time::Duration;

use chrono::Utc;
use serde_json::{Value, json};
use serenity::all::Member;

use database::{BlacklistRepository, CacheRepository, MemberRepository};

use crate::framework::Data;

/// Hypixel is pulled: a stored profile older than this is refreshed before use.
pub const HYPIXEL_MAX_AGE: Duration = Duration::from_secs(4 * 3600);
const PAUSE_NAME_UPDATES: &str = "pause_name_updates";

#[derive(Clone, Copy)]
pub enum HypixelRead {
    /// Refresh through coral-api when the stored profile is older than the maximum age.
    Fresh,
    /// The stored profile only, for bulk jobs that must not spend Hypixel calls.
    Stored,
}

pub struct CoralFacts {
    pub access: i16,
    pub trusted: bool,
    pub tags: Vec<String>,
    pub nickname_paused: bool,
}

pub async fn hypixel(data: &Data, uuid: &str, read: HypixelRead) -> Option<Value> {
    let cache = CacheRepository::new(data.db.pool());
    if matches!(read, HypixelRead::Fresh) && is_stale(&cache, uuid).await {
        match data.api.get_player_stats(uuid).await {
            Ok(response) => return response.hypixel,
            Err(e) => tracing::debug!("Hypixel refresh failed for {uuid}, using cache: {e}"),
        }
    }
    cache.get_latest_snapshot(uuid).await.ok().flatten()
}

pub async fn coral(data: &Data, discord_id: u64, uuid: &str) -> CoralFacts {
    let member = MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id as i64)
        .await
        .ok()
        .flatten();
    let tags = BlacklistRepository::new(data.db.pool())
        .get_active_tags(uuid)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.tag_type)
        .collect();
    CoralFacts {
        access: member.as_ref().map_or(0, |m| m.access_level),
        trusted: member.as_ref().is_some_and(|m| m.tag_granted),
        tags,
        nickname_paused: member
            .as_ref()
            .and_then(|m| m.config.get(PAUSE_NAME_UPDATES))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

pub fn context(hypixel: &Value, member: &Member, coral: &CoralFacts) -> Value {
    let mut ctx = hypixel.clone();

    if ctx.pointer("/achievements/bedwars_level").is_none() {
        ctx["achievements"]["bedwars_level"] = json!(0);
    }
    let active_bracket = ctx
        .pointer("/stats/Bedwars/active_prestige_bracket")
        .and_then(|v| v.as_str());
    let (bracket_open, bracket_close) = hypixel::nickname_bracket_glyphs(active_bracket);
    ctx["bedwars"] = json!({
        "bracket_open": bracket_open,
        "bracket_close": bracket_close,
    });

    ctx["discord"] = json!({
        "name": member.user.global_name.as_deref().unwrap_or(&member.user.name),
    });
    ctx["coral"] = json!({ "access": coral.access, "linked": true });
    ctx["standing"] = json!({ "trusted": coral.trusted });
    ctx["blacklist"] = blacklist(&coral.tags);
    ctx
}

fn blacklist(tags: &[String]) -> Value {
    let highest = tags
        .iter()
        .filter_map(|t| blacklist::lookup(t).map(|def| (def.priority, t.as_str())))
        .min_by_key(|(p, _)| *p)
        .map(|(_, name)| name);
    let mut bl = json!({ "tag": highest });
    for def in blacklist::all() {
        bl[def.name] = Value::Bool(tags.iter().any(|t| t == def.name));
    }
    bl
}

async fn is_stale(cache: &CacheRepository<'_>, uuid: &str) -> bool {
    match cache
        .get_latest_non_migration_timestamp(uuid)
        .await
        .ok()
        .flatten()
    {
        Some(ts) => (Utc::now() - ts).num_seconds() > HYPIXEL_MAX_AGE.as_secs() as i64,
        None => true,
    }
}
