use std::sync::Arc;

use clients::MojangClient;
use coral_redis::RedisPool;
use database::Database;

use crate::auth::AuthConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub auth: Arc<AuthConfig>,
    pub redis: RedisPool,
    pub redis_url: String,
    pub discord_token: Option<String>,
    pub sync_discord_token: Option<String>,
    pub home_guild_id: Option<i64>,
    pub review_forum_id: Option<i64>,
    pub http: reqwest::Client,
    pub mojang: MojangClient,
}

impl AppState {
    pub fn new(db: Database, redis: RedisPool, redis_url: String, auth: Arc<AuthConfig>) -> Self {
        let discord_token = env_non_empty("DISCORD_TOKEN");
        Self {
            db: Arc::new(db),
            auth,
            mojang: MojangClient::new(redis.connection()),
            redis,
            redis_url,
            sync_discord_token: env_non_empty("CORAL_SYNC_DISCORD_TOKEN")
                .or_else(|| discord_token.clone()),
            discord_token,
            home_guild_id: env_id("HOME_GUILD_ID"),
            review_forum_id: env_id("REVIEW_FORUM_ID"),
            http: reqwest::Client::new(),
        }
    }
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn env_id(name: &str) -> Option<i64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}
