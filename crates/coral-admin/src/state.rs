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
    pub discord_token: Option<String>,
    pub http: reqwest::Client,
    pub mojang: MojangClient,
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
        Self {
            db: Arc::new(db),
            owner_ids: Arc::new(owner_ids),
            redis,
            discord_token: std::env::var("DISCORD_TOKEN")
                .ok()
                .filter(|t| !t.is_empty()),
            http: reqwest::Client::new(),
            mojang: MojangClient::new(),
            oauth: Arc::new(oauth),
            session_secret,
        }
    }
}
