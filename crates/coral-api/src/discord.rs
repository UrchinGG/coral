use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

const CACHE_TTL_SECS: u64 = 900;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscordProfile {
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub discriminator: String,
}

pub struct DiscordResolver {
    http: Client,
    token: String,
    redis: ConnectionManager,
}

impl DiscordResolver {
    pub fn new(token: String, redis: ConnectionManager) -> Self {
        Self {
            http: Client::new(),
            token,
            redis,
        }
    }

    pub async fn is_guild_member(&self, guild_id: u64, user_id: u64) -> Option<bool> {
        let response = self
            .http
            .get(format!(
                "https://discord.com/api/v10/guilds/{guild_id}/members/{user_id}"
            ))
            .header("Authorization", format!("Bot {}", self.token))
            .send()
            .await
            .ok()?;

        match response.status() {
            StatusCode::OK => Some(true),
            StatusCode::NOT_FOUND => Some(false),
            _ => None,
        }
    }

    pub async fn resolve_username(&self, user_id: u64) -> Option<String> {
        self.resolve_profile(user_id)
            .await
            .map(|profile| profile.username)
    }

    pub async fn resolve_profile(&self, user_id: u64) -> Option<DiscordProfile> {
        let cache_key = format!("cache:discord:profile:{user_id}");

        if let Some(cached) = self.cached_profile(&cache_key).await {
            return Some(cached);
        }

        let profile = self
            .http
            .get(format!("https://discord.com/api/v10/users/{user_id}"))
            .header("Authorization", format!("Bot {}", self.token))
            .send()
            .await
            .ok()?
            .json::<DiscordProfile>()
            .await
            .ok()?;

        if let Ok(json) = serde_json::to_string(&profile) {
            let _: Result<(), _> = self
                .redis
                .clone()
                .set_ex(&cache_key, json, CACHE_TTL_SECS)
                .await;
        }

        Some(profile)
    }

    async fn cached_profile(&self, cache_key: &str) -> Option<DiscordProfile> {
        let json = self.redis.clone().get::<_, String>(cache_key).await.ok()?;
        serde_json::from_str(&json).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> serde_json::Result<DiscordProfile> {
        serde_json::from_str(json)
    }

    #[test]
    fn profile_from_discord_user_object() {
        let profile = parse(
            r#"{"id":"80351110224678912","username":"nelly","global_name":"Nelly","avatar":"8342729096ea3675442027381ff50dfe","discriminator":"0","public_flags":64}"#,
        )
        .unwrap();

        assert_eq!(
            profile,
            DiscordProfile {
                username: "nelly".into(),
                global_name: Some("Nelly".into()),
                avatar: Some("8342729096ea3675442027381ff50dfe".into()),
                discriminator: "0".into(),
            }
        );
    }

    #[test]
    fn profile_keeps_legacy_discriminator() {
        let profile = parse(
            r#"{"id":"1","username":"legacy","global_name":null,"avatar":null,"discriminator":"1337"}"#,
        )
        .unwrap();

        assert_eq!(profile.discriminator, "1337");
        assert_eq!(profile.global_name, None);
        assert_eq!(profile.avatar, None);
    }

    #[test]
    fn profile_requires_discriminator() {
        assert!(
            parse(r#"{"id":"1","username":"plain","global_name":null,"avatar":null}"#).is_err()
        );
    }

    #[test]
    fn error_body_is_not_a_profile() {
        assert!(parse(r#"{"message":"401: Unauthorized","code":0}"#).is_err());
    }

    #[test]
    fn profile_survives_cache_round_trip() {
        let profile = DiscordProfile {
            username: "nelly".into(),
            global_name: None,
            avatar: Some("a_8342729096ea3675442027381ff50dfe".into()),
            discriminator: "0".into(),
        };

        assert_eq!(
            parse(&serde_json::to_string(&profile).unwrap()).unwrap(),
            profile
        );
    }
}
