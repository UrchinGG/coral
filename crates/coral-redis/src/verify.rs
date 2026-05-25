use redis::{AsyncCommands, aio::ConnectionManager};
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
    redis::cmd("SET")
        .arg(format!("{KEY_PREFIX}{code}"))
        .arg(format!("{}:{username}", uuid.simple()))
        .arg("EX")
        .arg(CODE_TTL_SECS)
        .arg("NX")
        .query_async(conn)
        .await
}

pub async fn redeem_code(conn: &mut ConnectionManager, code: &str) -> Option<VerifiedPlayer> {
    let val: Option<String> = conn.get_del(format!("{KEY_PREFIX}{code}")).await.ok()?;
    let val = val?;
    let (uuid_str, username) = val.split_once(':')?;
    Some(VerifiedPlayer {
        uuid: Uuid::parse_str(uuid_str).ok()?,
        username: username.to_owned(),
    })
}
