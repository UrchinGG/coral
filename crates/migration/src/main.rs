use anyhow::Result;
use tracing::info;

mod blacklist;
mod cache;
mod client;
mod members;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    let skip_cache = args.iter().any(|a| a == "--skip-cache");
    let wipe = args.iter().any(|a| a == "--wipe");

    let coral_url = std::env::var("CORAL_API_URL")
        .unwrap_or_else(|_| "https://api.urchin.gg/v3".into());
    let api_key = std::env::var("CORAL_API_KEY")?;
    let mongodb_uri = std::env::var("MONGODB_URI")?;

    info!("Starting migration: MongoDB -> Coral API at {coral_url}");

    let client = client::CoralClient::new(&coral_url, &api_key);
    let mongo = mongodb::Client::with_uri_str(&mongodb_uri).await?;
    let db = mongo.database("urchindb");

    if wipe {
        info!("Wiping previous migration data...");
        let result = client.post(&serde_json::json!({"type": "wipe"})).await?;
        info!("Wiped: {result}");
    }

    info!("Migrating members...");
    let members_count = members::migrate(&db, &client).await?;
    info!("Migrated {members_count} members");

    info!("Migrating blacklist...");
    let blacklist_count = blacklist::migrate(&db, &client).await?;
    info!("Migrated {blacklist_count} blacklisted players");

    if skip_cache {
        info!("Skipping cache migration (--skip-cache)");
    } else {
        info!("Migrating cache...");
        let cache_count = cache::migrate(&db, &client).await?;
        info!("Migrated cache for {cache_count} players");
    }

    info!("Migration complete!");
    Ok(())
}
