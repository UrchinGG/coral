use redis::{AsyncCommands, aio::ConnectionManager};

use crate::ThreadAttachment;

const KEY_PREFIX: &str = "review_media:";
const MEDIA_TTL_SECS: u64 = 86_400;

pub async fn store(
    conn: &mut ConnectionManager,
    message_id: u64,
    attachments: &[ThreadAttachment],
) -> Result<(), redis::RedisError> {
    let payload =
        serde_json::to_string(attachments).expect("serializing plain strings cannot fail");
    conn.set_ex(key(message_id), payload, MEDIA_TTL_SECS).await
}

pub async fn load(
    conn: &mut ConnectionManager,
    message_id: u64,
) -> Result<Option<Vec<ThreadAttachment>>, redis::RedisError> {
    let payload: Option<String> = conn.get(key(message_id)).await?;
    Ok(payload.and_then(|p| serde_json::from_str(&p).ok()))
}

pub async fn forget(
    conn: &mut ConnectionManager,
    message_id: u64,
) -> Result<(), redis::RedisError> {
    conn.del(key(message_id)).await
}

fn key(message_id: u64) -> String {
    format!("{KEY_PREFIX}{message_id}")
}
