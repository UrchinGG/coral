use redis::{AsyncCommands, RedisResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::RedisPool;

pub const SESSION_TTL_SECS: u64 = 24 * 60 * 60;

const CHALLENGE_TTL_SECS: u64 = 60;
const SERVER_ID_BYTES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinecraftUser {
    pub uuid: Uuid,
    pub username: String,
}

pub async fn issue_challenge(redis: &RedisPool) -> RedisResult<String> {
    let server_id = hex::encode(rand::random::<[u8; SERVER_ID_BYTES]>());
    redis
        .connection()
        .set_ex::<_, _, ()>(challenge_key(&server_id), 1, CHALLENGE_TTL_SECS)
        .await?;
    Ok(server_id)
}

pub async fn redeem_challenge(redis: &RedisPool, server_id: &str) -> RedisResult<bool> {
    Ok(redis
        .connection()
        .del::<_, i64>(challenge_key(server_id))
        .await?
        == 1)
}

pub async fn start(redis: &RedisPool, user: &MinecraftUser) -> RedisResult<String> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    redis
        .connection()
        .set_ex::<_, _, ()>(
            session_key(&token),
            serde_json::to_string(user).expect("a minecraft user always serializes"),
            SESSION_TTL_SECS,
        )
        .await?;
    Ok(token)
}

pub async fn user(redis: &RedisPool, token: &str) -> Option<MinecraftUser> {
    serde_json::from_str(
        &redis
            .connection()
            .get::<_, String>(session_key(token))
            .await
            .ok()?,
    )
    .ok()
}

fn challenge_key(server_id: &str) -> String {
    format!("mc:challenge:{server_id}")
}

fn session_key(token: &str) -> String {
    format!("mc:session:{}", hex::encode(Sha256::digest(token)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_are_stored_under_the_token_hash() {
        let key = session_key("secret-token");
        assert!(key.starts_with("mc:session:"));
        assert!(!key.contains("secret-token"));
        assert_eq!(key, session_key("secret-token"));
    }
}
