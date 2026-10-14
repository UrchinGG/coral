use std::collections::HashMap;

use redis::aio::ConnectionManager;
use serde::Deserialize;
use uuid::Uuid;

const IDENTITY_PREFIX: &str = "mj:id:";
const STALE_PREFIX: &str = "mj:stale:";

#[derive(Deserialize)]
struct CachedIdentity {
    uuid: String,
}

pub async fn resolve(
    redis: &mut ConnectionManager,
    names: &[String],
) -> Result<HashMap<String, Uuid>, redis::RedisError> {
    let mut resolved = lookup(redis, IDENTITY_PREFIX, names).await?;
    let misses: Vec<String> = names
        .iter()
        .filter(|name| !resolved.contains_key(*name))
        .cloned()
        .collect();
    if !misses.is_empty() {
        resolved.extend(lookup(redis, STALE_PREFIX, &misses).await?);
    }
    Ok(resolved)
}

async fn lookup(
    redis: &mut ConnectionManager,
    prefix: &str,
    names: &[String],
) -> Result<HashMap<String, Uuid>, redis::RedisError> {
    let keys: Vec<String> = names.iter().map(|name| format!("{prefix}{name}")).collect();
    let values: Vec<Option<String>> = redis::cmd("MGET").arg(&keys).query_async(redis).await?;
    Ok(names
        .iter()
        .zip(values)
        .filter_map(|(name, value)| Some((name.clone(), cached_uuid(&value?)?)))
        .collect())
}

fn cached_uuid(raw: &str) -> Option<Uuid> {
    let identity: CachedIdentity = serde_json::from_str(raw).ok()?;
    Uuid::try_parse(&identity.uuid).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_identities_yield_their_uuid() {
        assert_eq!(
            cached_uuid(r#"{"uuid":"94589c87999b4a7ab3add2ae50711e64","username":"oJugg"}"#),
            Uuid::try_parse("94589c87999b4a7ab3add2ae50711e64").ok()
        );
        assert_eq!(cached_uuid("not json"), None);
    }
}
