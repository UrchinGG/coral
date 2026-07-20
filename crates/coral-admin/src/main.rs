use std::collections::HashSet;
use std::{env, net::SocketAddr};

use anyhow::Result;
use axum::{Router, middleware};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::state::{AppState, OAuthConfig};

mod audit;
mod auth;
mod discord;
mod identity;
mod routes;
mod serde_id;
mod session;
mod state;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let db = database::Database::connect(&env::var("DATABASE_URL").expect("DATABASE_URL required"))
        .await?;
    if let Err(e) = db.migrate().await {
        tracing::warn!("Migration skipped: {e}");
    }

    let owner_ids: HashSet<i64> = env::var("OWNER_IDS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    if owner_ids.is_empty() {
        tracing::warn!("OWNER_IDS is empty — every admin login will be rejected");
    }

    let redis = match env::var("REDIS_URL") {
        Ok(url) => match coral_redis::RedisPool::connect(&url).await {
            Ok(pool) => Some(pool),
            Err(e) => {
                tracing::warn!("Redis unavailable ({e}) — rate-limit panel disabled");
                None
            }
        },
        Err(_) => None,
    };

    let oauth = OAuthConfig {
        client_id: env::var("STARFISH_DISCORD_CLIENT_ID")
            .expect("STARFISH_DISCORD_CLIENT_ID required"),
        client_secret: env::var("STARFISH_DISCORD_CLIENT_SECRET")
            .expect("STARFISH_DISCORD_CLIENT_SECRET required"),
        base_url: env::var("ADMIN_BASE_URL").expect("ADMIN_BASE_URL required"),
    };
    let session_secret: [u8; 32] =
        hex::decode(env::var("ADMIN_SESSION_SECRET").expect("ADMIN_SESSION_SECRET required"))
            .expect("ADMIN_SESSION_SECRET must be valid hex")
            .try_into()
            .expect("ADMIN_SESSION_SECRET must be 32 bytes (64 hex chars)");

    let state = AppState::new(db, owner_ids, redis, oauth, session_secret);
    let app = Router::new()
        .merge(auth::router())
        .nest(
            "/api",
            routes::api_router().route_layer(middleware::from_fn_with_state(
                state.clone(),
                auth::require_owner,
            )),
        )
        .merge(routes::ui_router())
        .with_state(state);

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
