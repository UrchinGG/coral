use redis::{Client, aio::ConnectionManager};

#[derive(Clone)]
pub struct RedisPool {
    manager: ConnectionManager,
}

impl RedisPool {
    pub async fn connect(url: &str) -> Result<Self, redis::RedisError> {
        let manager = ConnectionManager::new(Client::open(url)?).await?;
        Ok(Self { manager })
    }

    pub fn connection(&self) -> ConnectionManager {
        self.manager.clone()
    }
}
