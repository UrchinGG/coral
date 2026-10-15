use redis::aio::{ConnectionManager, PubSub};
use redis::{Client, RedisResult};

#[derive(Clone)]
pub struct RedisPool {
    client: Client,
    manager: ConnectionManager,
}

impl RedisPool {
    pub async fn connect(url: &str) -> Result<Self, redis::RedisError> {
        let client = Client::open(url)?;
        let manager = ConnectionManager::new(client.clone()).await?;
        Ok(Self { client, manager })
    }

    pub fn connection(&self) -> ConnectionManager {
        self.manager.clone()
    }

    pub async fn pubsub(&self) -> RedisResult<PubSub> {
        self.client.get_async_pubsub().await
    }
}
