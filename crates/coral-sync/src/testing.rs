use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use database::Database;

use crate::api::CoralApiClient;
use crate::feeds::Feeds;
use crate::framework::Data;

/// `Data` on the test database and Redis, or `None` when they aren't configured.
pub async fn test_data() -> Option<Data> {
    dotenvy::dotenv().ok();
    let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
        .await
        .ok()?;
    let redis_url = std::env::var("REDIS_URL").ok()?;
    Some(Data {
        db: Arc::new(db),
        api: Arc::new(CoralApiClient::new("http://unused".into(), "unused".into())),
        owner_ids: Vec::new(),
        home_guild_id: None,
        redis: crate::jobs::connect_redis(&redis_url).await,
        redis_url,
        sync_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        job_cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        active_interactions: Arc::new(AtomicUsize::new(0)),
        feeds: Feeds::default(),
    })
}
