use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use reqwest::Client;
use serde::Deserialize;

const CACHE_TTL_SECS: u64 = 900;


#[derive(Deserialize)]
struct DiscordUser {
    username: String,
}


pub struct DiscordResolver {
    http: Client,
    token: String,
    redis: ConnectionManager,
}


impl DiscordResolver {
    pub fn new(token: String, redis: ConnectionManager) -> Self {
        Self { http: Client::new(), token, redis }
    }

    pub async fn resolve_username(&self, user_id: u64) -> Option<String> {
        let cache_key = format!("cache:discord:{user_id}");

        if let Ok(cached) = self.redis.clone().get::<_, String>(&cache_key).await {
            return Some(cached);
        }

        let user = self.http
            .get(format!("https://discord.com/api/v10/users/{user_id}"))
            .header("Authorization", format!("Bot {}", self.token))
            .send().await.ok()?
            .json::<DiscordUser>().await.ok()?;

        let _: Result<(), _> = self.redis.clone()
            .set_ex(&cache_key, &user.username, CACHE_TTL_SECS)
            .await;

        Some(user.username)
    }
}
