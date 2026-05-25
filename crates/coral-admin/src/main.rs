use std::{env, net::SocketAddr};

use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

mod routes;
mod state;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let db = database::Database::connect(&env::var("DATABASE_URL").expect("DATABASE_URL required"))
        .await?;
    let app = Router::new()
        .nest("/api", routes::api_router())
        .merge(routes::ui_router())
        .with_state(AppState::new(db));

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
