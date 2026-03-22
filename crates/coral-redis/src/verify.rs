use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use uuid::Uuid;

const KEY_PREFIX: &str = "verify:";
const CODE_TTL_SECS: u64 = 120;

pub struct VerifiedPlayer {
    pub uuid: Uuid,
    pub username: String,
}

pub async fn store_code(
    conn: &mut ConnectionManager,
    code: &str,
    uuid: Uuid,
    username: &str,
) -> Result<bool, redis::RedisError> {
    let key = format!("{KEY_PREFIX}{code}");
    let value = format!("{}:{username}", uuid.simple());
    redis::cmd("SET")
        .arg(&key)
        .arg(&value)
        .arg("EX")
        .arg(CODE_TTL_SECS)
        .arg("NX")
        .query_async(conn)
        .await
}

pub async fn redeem_code(
    conn: &mut ConnectionManager,
    code: &str,
) -> Option<VerifiedPlayer> {
    let key = format!("{KEY_PREFIX}{code}");
    let value: Option<String> = conn.get_del(&key).await.ok()?;
    let value = value?;
    let (uuid_str, username) = value.split_once(':')?;
    let uuid = Uuid::parse_str(uuid_str).ok()?;
    Some(VerifiedPlayer { uuid, username: username.to_string() })
}
