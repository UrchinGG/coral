use anyhow::Result;
use tracing::info;

use database::Database;

mod blacklist;
mod guilds;
mod members;
mod sink;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")?;
    let mongodb_uri = std::env::var("MONGODB_URI")?;
    let hypixel_api_key = std::env::var("HYPIXEL_API_KEY")?;

    info!("Starting migration: MongoDB -> Postgres (full replace, cutover mode)");

    let sink = sink::Sink::new(Database::connect(&database_url).await?.pool().clone());
    let mongo = mongodb::Client::with_uri_str(&mongodb_uri).await?;
    let db = mongo.database("urchindb");

    info!("Resetting members to a blank slate (api_key preserved)...");
    sink.reset_members_preserving_api_keys().await?;

    info!("Migrating members...");
    let members_count = members::migrate(&db, &sink).await?;
    info!("Migrated {members_count} members");

    info!("Wiping existing blacklist data...");
    sink.wipe_blacklist().await?;

    info!("Migrating blacklist...");
    let blacklist_count = blacklist::migrate(&db, &sink).await?;
    info!("Migrated {blacklist_count} blacklisted players");

    info!("Migrating guild subscriptions...");
    let guilds_count = guilds::migrate(&db, &sink, &hypixel_api_key).await?;
    info!("Migrated {guilds_count} guild subscriptions");

    info!("Migration complete!");
    Ok(())
}
