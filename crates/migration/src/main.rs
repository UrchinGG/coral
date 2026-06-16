use anyhow::Result;
use tracing::info;

use database::Database;

mod blacklist;
mod members;
mod sink;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let wipe = std::env::args().any(|a| a == "--wipe");

    let database_url = std::env::var("DATABASE_URL")?;
    let mongodb_uri = std::env::var("MONGODB_URI")?;

    info!("Starting migration: MongoDB -> Postgres");

    let sink = sink::Sink::new(Database::connect(&database_url).await?.pool().clone());
    let mongo = mongodb::Client::with_uri_str(&mongodb_uri).await?;
    let db = mongo.database("urchindb");

    if wipe {
        info!("Wiping previous blacklist data...");
        sink.wipe_blacklist().await?;
    }

    info!("Migrating members...");
    let members_count = members::migrate(&db, &sink).await?;
    info!("Migrated {members_count} members");

    info!("Migrating blacklist...");
    let blacklist_count = blacklist::migrate(&db, &sink).await?;
    info!("Migrated {blacklist_count} blacklisted players");

    info!("Migration complete!");
    Ok(())
}
