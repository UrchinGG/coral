use redis::AsyncCommands;

use crate::framework::Data;

const CACHE_SECS: u64 = 60 * 60;

pub async fn face_png(data: &Data, uuid: &str, size: u32) -> Option<Vec<u8>> {
    let key = format!("face_png:{uuid}:{size}");
    let mut redis = data.redis.connection();
    if let Ok(Some(png)) = redis.get::<_, Option<Vec<u8>>>(&key).await {
        return Some(png);
    }
    let png = data.skin_provider.fetch_face(uuid, size).await?;
    if let Err(e) = redis.set_ex::<_, _, ()>(&key, &png, CACHE_SECS).await {
        tracing::warn!("Failed to cache face for {uuid}: {e}");
    }
    Some(png)
}
