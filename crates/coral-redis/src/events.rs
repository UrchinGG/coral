use futures_util::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::RedisPool;

const CHANNEL: &str = "blacklist:events";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BlacklistEvent {
    TagAdded {
        uuid: String,
        tag_id: i64,
        added_by: i64,
        #[serde(default)]
        silent: bool,
    },
    TagOverwritten {
        uuid: String,
        old_tag_id: i64,
        old_tag_type: String,
        old_reason: String,
        new_tag_id: i64,
        overwritten_by: i64,
        #[serde(default)]
        silent: bool,
    },
    TagRemoved {
        uuid: String,
        tag_id: i64,
        removed_by: i64,
        #[serde(default)]
        silent: bool,
    },
    TagEdited {
        uuid: String,
        tag_id: i64,
        old_tag_type: String,
        old_reason: String,
        edited_by: i64,
    },
    PlayerLocked {
        uuid: String,
        locked_by: i64,
        reason: String,
    },
    PlayerUnlocked {
        uuid: String,
        unlocked_by: i64,
    },
}

#[derive(Clone)]
pub struct EventPublisher {
    pool: RedisPool,
}

impl EventPublisher {
    pub fn new(pool: RedisPool) -> Self {
        Self { pool }
    }

    pub async fn publish(&self, event: &BlacklistEvent) {
        let Ok(payload) = serde_json::to_string(event).inspect_err(|e| {
            tracing::error!("Failed to serialize blacklist event: {e}");
        }) else {
            return;
        };
        if let Err(e) = self
            .pool
            .connection()
            .publish::<_, _, ()>(CHANNEL, &payload)
            .await
        {
            tracing::error!("Failed to publish blacklist event: {e}");
        }
    }
}

pub struct EventSubscriber;

impl EventSubscriber {
    pub async fn run<F, Fut>(redis_url: &str, handler: F) -> Result<(), redis::RedisError>
    where
        F: Fn(BlacklistEvent) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let client = redis::Client::open(redis_url)?;
        let mut pubsub = client.get_async_pubsub().await?;
        pubsub.subscribe(CHANNEL).await?;

        let mut stream = pubsub.into_on_message();
        while let Some(msg) = stream.next().await {
            let Ok(payload) = msg.get_payload::<String>().inspect_err(|e| {
                tracing::error!("Failed to read event payload: {e}");
            }) else {
                continue;
            };
            match serde_json::from_str::<BlacklistEvent>(&payload) {
                Ok(event) => handler(event).await,
                Err(e) => tracing::error!("Failed to deserialize event: {e}"),
            }
        }

        Ok(())
    }
}

const SYNC_CHANNEL: &str = "sync:events";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncEvent {
    SyncUser { discord_id: u64 },
}

#[derive(Clone)]
pub struct SyncEventPublisher {
    pool: RedisPool,
}

impl SyncEventPublisher {
    pub fn new(pool: RedisPool) -> Self {
        Self { pool }
    }

    pub async fn publish(&self, event: &SyncEvent) {
        let Ok(payload) = serde_json::to_string(event).inspect_err(|e| {
            tracing::error!("Failed to serialize sync event: {e}");
        }) else {
            return;
        };
        if let Err(e) = self
            .pool
            .connection()
            .publish::<_, _, ()>(SYNC_CHANNEL, &payload)
            .await
        {
            tracing::error!("Failed to publish sync event: {e}");
        }
    }
}

pub struct SyncEventSubscriber;

impl SyncEventSubscriber {
    pub async fn run<F, Fut>(redis_url: &str, handler: F) -> Result<(), redis::RedisError>
    where
        F: Fn(SyncEvent) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let client = redis::Client::open(redis_url)?;
        let mut pubsub = client.get_async_pubsub().await?;
        pubsub.subscribe(SYNC_CHANNEL).await?;

        let mut stream = pubsub.into_on_message();
        while let Some(msg) = stream.next().await {
            let Ok(payload) = msg.get_payload::<String>().inspect_err(|e| {
                tracing::error!("Failed to read sync event payload: {e}");
            }) else {
                continue;
            };
            match serde_json::from_str::<SyncEvent>(&payload) {
                Ok(event) => handler(event).await,
                Err(e) => tracing::error!("Failed to deserialize sync event: {e}"),
            }
        }

        Ok(())
    }
}
