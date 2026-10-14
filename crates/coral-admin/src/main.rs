use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::{env, net::SocketAddr};

use anyhow::{Context, Result, anyhow, bail};
use coral_redis::RedisPool;
use tokio::net::TcpListener;
use tokio::time;
use tracing_subscriber::EnvFilter;
use url::Url;

use crate::auth::{AuthConfig, DiscordOAuth};
use crate::epoch::SessionEpoch;
use crate::session::AuthKeys;
use crate::state::AppState;

mod audit;
mod auth;
mod discord;
mod epoch;
mod identity;
mod perimeter;
mod routes;
mod security;
mod serde_id;
mod session;
mod state;

const SESSION_SECRET_LEN: usize = 32;
const REDIS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const WEB_SCHEMES: [&str; 2] = ["http", "https"];

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let db = database::Database::connect(&required_env("DATABASE_URL")?).await?;
    db.migrate_tolerating_newer_schema().await?;

    let redis_url = required_env("REDIS_URL")?;
    let redis = time::timeout(REDIS_CONNECT_TIMEOUT, RedisPool::connect(&redis_url))
        .await
        .with_context(|| {
            format!("Redis did not accept a connection within {REDIS_CONNECT_TIMEOUT:?}")
        })?
        .context("connect to Redis, which holds the admin session epoch")?;
    let auth = Arc::new(auth_config(&redis).await?);
    auth.epoch.clone().keep_fresh();

    let state = AppState::new(db, redis, redis_url, auth.clone());
    let app = perimeter::router(auth, routes::api_router()).with_state(state);

    let port: u16 = env::var("ADMIN_PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .expect("ADMIN_PORT must be a number");
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("coral-admin listening on http://{}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn auth_config(redis: &RedisPool) -> Result<AuthConfig> {
    let owner_ids = owner_ids(&required_env("OWNER_IDS")?)?;
    let base_url = admin_base_url()?;
    let secret: [u8; SESSION_SECRET_LEN] = hex::decode(required_env("ADMIN_SESSION_SECRET")?)
        .context("ADMIN_SESSION_SECRET must be valid hex")?
        .try_into()
        .map_err(|_| {
            anyhow!("ADMIN_SESSION_SECRET must be {SESSION_SECRET_LEN} hex-encoded bytes")
        })?;
    let discord = DiscordOAuth::new(
        required_env("STARFISH_DISCORD_CLIENT_ID")?,
        required_env("STARFISH_DISCORD_CLIENT_SECRET")?,
        &base_url,
    )?;
    let epoch = SessionEpoch::load(Arc::new(redis.clone())).await?;
    Ok(AuthConfig {
        owner_ids,
        base_url,
        keys: AuthKeys::derive(&secret),
        epoch: Arc::new(epoch),
        discord,
    })
}

fn admin_base_url() -> Result<Url> {
    let base_url = Url::parse(&required_env("ADMIN_BASE_URL")?)
        .context("ADMIN_BASE_URL must be an absolute URL")?;
    if !WEB_SCHEMES.contains(&base_url.scheme()) {
        bail!("ADMIN_BASE_URL must use http or https");
    }
    Ok(base_url)
}

fn owner_ids(raw: &str) -> Result<HashSet<i64>> {
    let owner_ids = raw
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse()
                .with_context(|| format!("OWNER_IDS entry {entry:?} is not a Discord id"))
        })
        .collect::<Result<HashSet<i64>>>()?;
    if owner_ids.is_empty() {
        bail!("OWNER_IDS must list at least one Discord id");
    }
    Ok(owner_ids)
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} required"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_ids_keep_every_listed_id() {
        assert_eq!(owner_ids(" 1, 2 ,3,").unwrap(), HashSet::from([1, 2, 3]));
    }

    #[test]
    fn owner_ids_reject_a_malformed_entry() {
        let error = owner_ids("123,45x").unwrap_err();
        assert!(error.to_string().contains("\"45x\""), "{error}");
    }

    #[test]
    fn owner_ids_must_name_at_least_one_owner() {
        for raw in ["", " , "] {
            assert!(owner_ids(raw).is_err(), "{raw:?}");
        }
    }
}
