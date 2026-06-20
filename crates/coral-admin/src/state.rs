use std::collections::HashSet;
use std::sync::Arc;

use coral_redis::RedisPool;
use database::Database;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub owner_ids: Arc<HashSet<i64>>,
    pub redis: Option<RedisPool>,
    pub discord_token: Option<String>,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(db: Database, owner_ids: HashSet<i64>, redis: Option<RedisPool>) -> Self {
        Self {
            db: Arc::new(db),
            owner_ids: Arc::new(owner_ids),
            redis,
            discord_token: std::env::var("DISCORD_TOKEN")
                .ok()
                .filter(|t| !t.is_empty()),
            http: reqwest::Client::new(),
        }
    }
}
