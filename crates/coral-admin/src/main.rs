use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::{env, net::SocketAddr};

use anyhow::{Context, Result, bail};
use coral_redis::RedisPool;
use tokio::net::TcpListener;
use tokio::time;
use tracing_subscriber::EnvFilter;
use url::Url;

use crate::auth::AuthConfig;
use crate::discord::DiscordContext;
use crate::state::{AppState, Connections};

mod audit;
mod auth;
mod boot;
mod coverage;
mod discord;
mod error;
mod extensions;
mod flags;
mod jobs;
mod layers;
mod maintenance;
mod names;
mod perimeter;
mod players;
mod query;
mod refs;
mod requests;
mod rollups;
mod routes;
mod search;
mod security;
mod serde_id;
mod serde_ts;
mod session;
mod state;
mod traffic;
mod views;

const REDIS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const WEB_SCHEMES: [&str; 2] = ["http", "https"];

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let database_url = required_env("DATABASE_URL")?;
    let redis_url = required_env("REDIS_URL")?;
    let redis = time::timeout(REDIS_CONNECT_TIMEOUT, RedisPool::connect(&redis_url))
        .await
        .with_context(|| {
            format!("Redis did not accept a connection within {REDIS_CONNECT_TIMEOUT:?}")
        })?
        .context("connect to Redis, which holds the sign-in sessions")?;
    let connections = Connections::open(&database_url, redis.clone()).await?;
    connections
        .jobs_db
        .migrate_tolerating_newer_schema()
        .await?;
    let auth = Arc::new(auth_config(&redis)?);

    let discord = DiscordContext::from_env()?;
    let (state, name_workers) = AppState::start(connections, auth.clone(), discord).await?;
    name_workers.spawn();
    state.rollups.clone().supervise(state.jobs.clone());
    let api = routes::api_router(state.clone());
    let app = perimeter::router(auth, state.boot.clone(), api);

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

fn auth_config(redis: &RedisPool) -> Result<AuthConfig> {
    Ok(AuthConfig {
        owner_ids: owner_ids(&required_env("OWNER_IDS")?)?,
        base_url: admin_base_url()?,
        sessions: Arc::new(redis.clone()),
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
