use std::env;

use mc_verify::VerifyServer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let redis_url = env::var("REDIS_URL").expect("REDIS_URL required");
    let address = env::var("VERIFY_SERVER_ADDRESS").unwrap_or_else(|_| "0.0.0.0:25565".into());

    let client = redis::Client::open(redis_url)?;
    let redis = redis::aio::ConnectionManager::new(client).await?;

    VerifyServer::new(&address, redis)
        .disconnect_message(|code| {
            format!(
                "Your verification code is: §a§l{code}\n\n\
                 §rUse §f/link §ror §f/dashboard §rin Discord to enter this code.\n\
                 §7Expires in 2 minutes."
            )
        })
        .start()
        .await?;

    Ok(())
}
