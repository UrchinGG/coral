use std::env;

use mc_verify::VerifyServer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let api_url = env::var("CORAL_API_URL").expect("CORAL_API_URL required");
    let api_key = env::var("INTERNAL_API_KEY").expect("INTERNAL_API_KEY required");
    let address = env::var("VERIFY_SERVER_ADDRESS").unwrap_or_else(|_| "0.0.0.0:25565".into());

    VerifyServer::new(&address, &api_url, &api_key)
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
