//! What Coral knows about a member, by source. Each source fills its own namespace
//! of the context the engine reads, except Hypixel, which still sits at the root
//! until the one-time template rewrite moves it under `hypixel.*`.
//!
//! External sources are rows, not code: each speaks the same protocol, keyed by
//! Discord ID. `GET {base}/members/{discord_id}` answers a JSON object or 404.

use std::sync::LazyLock;
use std::time::Duration;

use chrono::Utc;
use reqwest::StatusCode;
use serde_json::{Value, json};

use database::{
    BlacklistRepository, CacheRepository, MemberRepository, SyncSource, SyncSourceRepository,
};

use crate::framework::Data;

/// Hypixel is pulled: a stored profile older than this is refreshed before use.
pub const HYPIXEL_MAX_AGE: Duration = Duration::from_secs(4 * 3600);
const PAUSE_NAME_UPDATES: &str = "pause_name_updates";
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SOURCE_NAME_LEN: usize = 32;
/// Namespaces Coral fills itself, plus the Hypixel root fields templates use most.
const RESERVED_NAMES: &[&str] = &[
    "achievements",
    "bedwars",
    "blacklist",
    "coral",
    "discord",
    "displayname",
    "hypixel",
    "standing",
    "stats",
];

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(LOOKUP_TIMEOUT)
        .user_agent("coral-sync")
        .build()
        .expect("failed to create HTTP client")
});

#[derive(Clone, Copy)]
pub enum HypixelRead {
    /// Refresh through coral-api when the stored profile is older than the maximum age.
    Fresh,
    /// The stored profile only, for bulk jobs that must not spend Hypixel calls.
    Stored,
}

/// A source a server uses has never answered for this member, and the last attempt
/// failed. The member is skipped this round: unknown is not false.
#[derive(Debug)]
pub struct Unavailable;

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

/// The source's value from the cache, refreshed first when it is stale, or older than
/// the source's maximum age for a source without a feed. A failed refresh falls back
/// to the cached value, however old.
pub async fn external(
    data: &Data,
    source: &SyncSource,
    discord_id: u64,
) -> Result<Option<Value>, Unavailable> {
    let repo = SyncSourceRepository::new(data.db.pool());
    let cached = repo
        .cached(source.id, discord_id as i64)
        .await
        .map_err(|e| tracing::warn!("read the {} cache: {e}", source.name))
        .ok()
        .flatten();
    if let Some(cached) = &cached
        && !cached.stale
        && (data.feeds.is_connected(source.id)
            || (Utc::now() - cached.fetched_at).num_seconds() < i64::from(source.max_age_secs))
    {
        return Ok(cached.value.clone());
    }
    match lookup(source, discord_id).await {
        Ok(value) => {
            if let Err(e) = repo
                .store(source.id, discord_id as i64, value.as_ref())
                .await
            {
                tracing::warn!("cache a {} value: {e}", source.name);
            }
            Ok(value)
        }
        Err(e) => {
            tracing::debug!("{} lookup for {discord_id} failed: {e}", source.name);
            cached.map(|cached| cached.value).ok_or(Unavailable)
        }
    }
}

pub async fn lookup(source: &SyncSource, discord_id: u64) -> Result<Option<Value>, String> {
    let url = format!(
        "{}/members/{discord_id}",
        source.base_url.trim_end_matches('/')
    );
    let mut request = HTTP.get(url);
    if let Some(key) = &source.api_key {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    match response.status() {
        StatusCode::NOT_FOUND => Ok(None),
        status if status.is_success() => match response.json::<Value>().await {
            Ok(value @ Value::Object(_)) => Ok(Some(value)),
            Ok(_) => Err("the answer is not a JSON object".into()),
            Err(e) => Err(e.to_string()),
        },
        status => Err(format!("answered {status}")),
    }
}

pub fn valid_source_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && name.len() <= MAX_SOURCE_NAME_LEN
        && !RESERVED_NAMES.contains(&name)
}

/// Puts a source's value under its name. While Hypixel still sits at the root, a
/// name that collides with a Hypixel field is skipped rather than overwrite it.
pub fn add_external(context: &mut Value, name: &str, value: Value) {
    if context.get(name).is_some() {
        tracing::warn!("source {name} collides with a context field and was left out");
        return;
    }
    context[name] = value;
}

pub fn context(hypixel: &Value, discord_name: Option<&str>, coral: &CoralFacts) -> Value {
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

    ctx["discord"] = json!({ "name": discord_name });
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
