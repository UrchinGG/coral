mod auth;
mod codes;
mod connection;
mod encryption;
mod protocol;

use std::sync::Arc;

use base64::Engine;
use tokio::net::TcpListener;
use tracing::info;

use crate::connection::ServerState;

const DEFAULT_MOTD: &str = "Coral Account Linking\nJoin and copy the provided 4-digit code";
const DEFAULT_ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

type FormatFn = Box<dyn Fn(&str) -> String + Send + Sync>;

pub struct VerifyServer {
    address: String,
    api_url: String,
    api_key: String,
    disconnect_message: Option<FormatFn>,
}

impl VerifyServer {
    pub fn new(
        address: impl Into<String>,
        api_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            address: address.into(),
            api_url: api_url.into(),
            api_key: api_key.into(),
            disconnect_message: None,
        }
    }

    pub fn disconnect_message(
        mut self,
        f: impl Fn(&str) -> String + Send + Sync + 'static,
    ) -> Self {
        self.disconnect_message = Some(Box::new(f));
        self
    }

    pub async fn start(self) -> std::io::Result<()> {
        info!("generating RSA keypair...");
        let http = reqwest::Client::new();
        let state = Arc::new(ServerState {
            key: encryption::ServerKey::generate(),
            codes: codes::CodeStore::new(http.clone(), self.api_url, self.api_key),
            http,
            motd: DEFAULT_MOTD.into(),
            server_icon: Some(base64::engine::general_purpose::STANDARD.encode(DEFAULT_ICON_PNG)),
            format_disconnect: self.disconnect_message.unwrap_or_else(|| {
                Box::new(|code| {
                    format!("Your verification code is: §a§l{code}\n\n§7Expires in 2 minutes.")
                })
            }),
        });
        let listener = TcpListener::bind(&self.address).await?;
        info!("verify server listening on {}", self.address);
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(connection::handle_connection(stream, Arc::clone(&state)));
                }
                Err(e) => tracing::error!("accept failed: {e}"),
            }
        }
    }
}
