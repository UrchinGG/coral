#[cfg(unix)]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serenity::all::*;
use tracing_subscriber::EnvFilter;

use database::Database;

use coral_sync::api::CoralApiClient;
use coral_sync::framework::{Data, Handler};

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let data = init_data().await?;
    let mut client = build_client(data).await?;
    tracing::info!("Starting Coral Sync");
    client.start().await?;
    Ok(())
}

fn init_logging() {
    dotenvy::dotenv().ok();
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,serenity=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn init_data() -> Result<Data> {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL required");
    let api_url = std::env::var("CORAL_API_URL").expect("CORAL_API_URL required");
    let api_key = std::env::var("INTERNAL_API_KEY").expect("INTERNAL_API_KEY required");

    let db = Database::connect(&database_url).await?;
    if let Err(e) = db.migrate().await {
        tracing::warn!("Migration skipped: {e}");
    }
    let api = CoralApiClient::new(api_url, api_key);

    let home_guild_id = parse_guild_id("HOME_GUILD_ID");

    Ok(Data {
        db: Arc::new(db),
        api: Arc::new(api),
        owner_ids: parse_owner_ids(),
        home_guild_id,
        redis_url,
        sync_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        sync_cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        active_interactions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    })
}

fn parse_owner_ids() -> Vec<u64> {
    std::env::var("OWNER_IDS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

fn parse_guild_id(name: &str) -> Option<GuildId> {
    let raw = std::env::var(name).ok()?;
    let id = raw.trim().parse::<u64>().ok()?;
    Some(GuildId::new(id))
}

async fn build_client(data: Data) -> Result<Client> {
    let token =
        Token::from_env("CORAL_SYNC_DISCORD_TOKEN").expect("Invalid CORAL_SYNC_DISCORD_TOKEN");
    let intents =
        GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES | GatewayIntents::GUILD_MEMBERS;

    let mut cache_settings = serenity::cache::Settings::default();
    cache_settings.cache_guilds = false;
    cache_settings.cache_users = false;
    cache_settings.cache_channels = false;

    Ok(Client::builder(token, intents)
        .cache_settings(cache_settings)
        .event_handler(Arc::new(Handler::new(data)))
        .await?)
}
