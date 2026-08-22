use std::collections::HashSet;
use std::sync::Arc;

use clients::MojangClient;
use coral_redis::RedisPool;
use database::Database;

#[derive(Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub base_url: String,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub owner_ids: Arc<HashSet<i64>>,
    pub redis: Option<RedisPool>,
    pub redis_url: Option<String>,
    pub discord_token: Option<String>,
    pub sync_discord_token: Option<String>,
    pub home_guild_id: Option<i64>,
    pub review_forum_id: Option<i64>,
    pub http: reqwest::Client,
    pub mojang: Option<MojangClient>,
    pub oauth: Arc<OAuthConfig>,
    pub session_secret: [u8; 32],
}

impl AppState {
    pub fn new(
        db: Database,
        owner_ids: HashSet<i64>,
        redis: Option<RedisPool>,
        oauth: OAuthConfig,
        session_secret: [u8; 32],
    ) -> Self {
        let discord_token = env_non_empty("DISCORD_TOKEN");
        let mojang = redis.as_ref().map(|r| MojangClient::new(r.connection()));
        Self {
            db: Arc::new(db),
            owner_ids: Arc::new(owner_ids),
            redis,
            redis_url: env_non_empty("REDIS_URL"),
            sync_discord_token: env_non_empty("CORAL_SYNC_DISCORD_TOKEN")
                .or_else(|| discord_token.clone()),
            discord_token,
            home_guild_id: env_id("HOME_GUILD_ID"),
            review_forum_id: env_id("REVIEW_FORUM_ID"),
            http: reqwest::Client::new(),
            mojang,
            oauth: Arc::new(oauth),
            session_secret,
        }
    }
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn env_id(name: &str) -> Option<i64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}
