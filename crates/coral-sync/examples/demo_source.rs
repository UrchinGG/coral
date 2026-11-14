//! Runs the demo data source. Register it with the sync as `demo`, pick it for a test
//! server, then type changes here:
//!
//!   set <discord_id> <json object>
//!   unset <discord_id>
//!   forget            (feeds resuming from an older seq get a reset)

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;

use coral_sync::demo_source::DemoSource;

const DEFAULT_PORT: &str = "9000";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_PORT.into());
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    let demo = DemoSource::new();
    tokio::spawn(demo.clone().serve(listener));
    println!("demo source on port {port}");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let mut words = line.splitn(3, ' ');
        match (
            words.next(),
            words.next().and_then(|id| id.parse().ok()),
            words.next(),
        ) {
            (Some("set"), Some(id), Some(json)) => match serde_json::from_str(json) {
                Ok(value) => demo.set(id, Some(value)),
                Err(e) => println!("not JSON: {e}"),
            },
            (Some("unset"), Some(id), None) => demo.set(id, None),
            (Some("forget"), None, None) => demo.forget_history(),
            _ => println!("set <discord_id> <json> | unset <discord_id> | forget"),
        }
    }
    Ok(())
}
