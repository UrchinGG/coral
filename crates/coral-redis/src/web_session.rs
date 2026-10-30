use redis::{AsyncCommands, RedisResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::RedisPool;

pub const COOKIE: &str = "__Secure-urchin_session";

const DOMAIN: &str = "urchin.gg";
const TTL_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Serialize, Deserialize)]
pub struct WebUser {
    pub discord_id: i64,
    pub name: String,
    pub username: String,
    pub avatar: String,
}

pub async fn start(redis: &RedisPool, user: &WebUser) -> RedisResult<String> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    let user = serde_json::to_string(user).expect("a web user always serializes");
    redis
        .connection()
        .set_ex::<_, _, ()>(key(&token), user, TTL_SECS)
        .await?;
    Ok(token)
}

pub async fn user(redis: &RedisPool, token: &str) -> Option<WebUser> {
    let user: String = redis.connection().get(key(token)).await.ok()?;
    serde_json::from_str(&user).ok()
}

pub async fn end(redis: &RedisPool, token: &str) -> RedisResult<()> {
    redis.connection().del(key(token)).await
}

pub fn cookie(token: &str) -> String {
    format!(
        "{COOKIE}={token}; Domain={DOMAIN}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={TTL_SECS}"
    )
}

pub fn cleared_cookie() -> String {
    format!("{COOKIE}=; Domain={DOMAIN}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0")
}

fn key(token: &str) -> String {
    format!("web:session:{}", hex::encode(Sha256::digest(token)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_covers_the_whole_site() {
        assert_eq!(
            cookie("t"),
            "__Secure-urchin_session=t; Domain=urchin.gg; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000"
        );
    }

    #[test]
    fn stored_session_keys_never_contain_the_token() {
        let token = "secret-token";
        assert!(!key(token).contains(token));
        assert_eq!(key(token), key(token));
    }
}
