//! The shadow run for one server against a live database and Discord, reading only:
//! its stored template and rules are rewritten in memory to the `hypixel.*` layout,
//! every member is planned as a resync would, and nothing is written.
//!
//!   DATABASE_URL=… CORAL_SYNC_DISCORD_TOKEN=… cargo run -p coral-sync --example shadow <guild_id>

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serenity::all::{GuildId, Http, Token};

use coral_sync::api::CoralApiClient;
use coral_sync::engine::Settings;
use coral_sync::feeds::Feeds;
use coral_sync::framework::Data;
use coral_sync::{layout, service};
use database::{Database, GuildConfigRepository, GuildRoleRule};

#[tokio::main]
async fn main() -> Result<()> {
    let guild_id: u64 = std::env::args()
        .nth(1)
        .context("usage: shadow <guild_id>")?
        .parse()?;
    let token: Token = std::env::var("CORAL_SYNC_DISCORD_TOKEN")?.parse()?;
    let db = Database::connect(&std::env::var("DATABASE_URL")?).await?;

    let repo = GuildConfigRepository::new(db.pool());
    let config = repo
        .get(guild_id as i64)
        .await?
        .context("that server has no sync config")?;
    let rules: Vec<GuildRoleRule> = repo
        .get_role_rules(guild_id as i64)
        .await?
        .into_iter()
        .map(|rule| GuildRoleRule {
            condition: layout::rewrite_condition(&rule.condition, &[]),
            ..rule
        })
        .collect();
    let mut config = config;
    config.nickname_template = config
        .nickname_template
        .map(|template| layout::rewrite_template(&template, &[]));

    let data = Data {
        db: Arc::new(db),
        api: Arc::new(CoralApiClient::new("http://unused".into(), String::new())),
        owner_ids: Vec::new(),
        home_guild_id: None,
        redis_url: String::new(),
        redis: None,
        sync_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        job_cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        active_interactions: Arc::new(AtomicUsize::new(0)),
        feeds: Feeds::default(),
    };
    let http = Http::new(token);
    let shadow = service::shadow_settings(
        &http,
        &data,
        GuildId::new(guild_id),
        Settings::new(&config, &rules),
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&shadow)?);
    Ok(())
}
