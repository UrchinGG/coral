#[cfg(unix)]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use serenity::all::*;
use tracing_subscriber::EnvFilter;

use clients::{LocalSkinProvider, SkinProvider};
use coral_redis::{EventPublisher, RedisPool, SyncEventPublisher};
use database::Database;

use coral_bot::api::CoralApiClient;
use coral_bot::framework::{AccessRank, Data, Handler};

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let data = init_data().await?;
    let mut client = build_client(data).await?;
    tracing::info!("Starting Coral Bot");
    client.start().await?;
    Ok(())
}

fn init_logging() {
    dotenvy::dotenv().ok();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,wgpu_core=error,wgpu_hal=error,naga=error,serenity=warn")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn init_data() -> Result<Data> {
    render::init_canvas();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL required");
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL required");
    let api_url = env::var("CORAL_API_URL").expect("CORAL_API_URL required");
    let api_key = env::var("INTERNAL_API_KEY").expect("INTERNAL_API_KEY required");

    let db = Database::connect(&database_url).await?;
    if let Err(e) = db.migrate().await {
        tracing::warn!("Migration skipped: {e}");
    }
    let redis = RedisPool::connect(&redis_url).await?;
    let event_publisher = EventPublisher::new(redis.clone());
    let sync_event_publisher = SyncEventPublisher::new(redis.clone());
    let api = CoralApiClient::new(api_url, api_key);
    let skin_provider: Arc<dyn SkinProvider> = Arc::new(
        LocalSkinProvider::new(redis.connection()).expect("Failed to initialize skin renderer"),
    );

    let review_forum_id = parse_channel_id("REVIEW_FORUM_ID");
    let evidence_forum_id = parse_channel_id("EVIDENCE_FORUM_ID");
    let blacklist_channel_id = parse_channel_id("BLACKLIST_CHANNEL_ID");
    let mod_channel_id = parse_channel_id("MOD_CHANNEL_ID");

    tracing::info!(
        "Channels: blacklist={:?} mod={:?} review={:?} evidence={:?}",
        blacklist_channel_id,
        mod_channel_id,
        review_forum_id,
        evidence_forum_id,
    );

    Ok(Data {
        db: Arc::new(db),
        api: Arc::new(api),
        skin_provider,
        owner_ids: parse_owner_ids(),
        home_guild_id: parse_guild_id("HOME_GUILD_ID"),
        blacklist_channel_id,
        mod_channel_id,
        review_forum_id,
        evidence_forum_id,
        redis,
        redis_url,
        event_publisher,
        sync_event_publisher,
        bedwars_images: Arc::new(Mutex::new(HashMap::new())),
        duels_images: Arc::new(Mutex::new(HashMap::new())),
        session_images: Arc::new(Mutex::new(HashMap::new())),
        session_duels_images: Arc::new(Mutex::new(HashMap::new())),
        pending_overwrites: Arc::new(Mutex::new(HashMap::new())),
        pending_review_votes: Arc::new(Mutex::new(HashMap::new())),
        evidence_threads: Arc::new(RwLock::new(HashMap::new())),
        sync_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        active_interactions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        vote_min_rank: env::var("VOTE_MIN_RANK")
            .ok()
            .and_then(|v| v.parse::<i16>().ok())
            .map(AccessRank::from_level)
            .unwrap_or(AccessRank::Trusted),
        vote_messages: Arc::new(Mutex::new(HashMap::new())),
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        info_cache: Arc::new(Mutex::new(Default::default())),
    })
}

fn parse_owner_ids() -> Vec<u64> {
    env::var("OWNER_IDS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

fn parse_channel_id(name: &str) -> Option<ChannelId> {
    let raw = env::var(name).ok()?;
    let id = raw.trim().parse::<u64>().ok()?;
    Some(ChannelId::new(id))
}

fn parse_guild_id(name: &str) -> Option<GuildId> {
    let raw = env::var(name).ok()?;
    let id = raw.trim().parse::<u64>().ok()?;
    Some(GuildId::new(id))
}

async fn build_client(data: Data) -> Result<Client> {
    let token = Token::from_env("DISCORD_TOKEN").expect("Invalid DISCORD_TOKEN");
    let intents = GatewayIntents::GUILDS;

    let mut cache_settings = serenity::cache::Settings::default();
    cache_settings.cache_guilds = false;
    cache_settings.cache_users = false;
    cache_settings.cache_channels = false;

    Ok(Client::builder(token, intents)
        .cache_settings(cache_settings)
        .event_handler(Arc::new(Handler::new(data)))
        .await?)
}
