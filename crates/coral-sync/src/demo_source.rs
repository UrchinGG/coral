//! A data source speaking the sync's protocol, for tests and for proving the sync on
//! a test server (`cargo run -p coral-sync --example demo_source`). Values live in
//! memory; every change is numbered and pushed to connected feeds.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::SinkExt;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

const HEAD_LIMIT: usize = 8192;
const FEED_BUFFER: usize = 256;

#[derive(Clone)]
pub struct DemoSource {
    state: Arc<Mutex<State>>,
    changes: broadcast::Sender<String>,
}

#[derive(Default)]
struct State {
    members: HashMap<u64, Value>,
    log: Vec<(i64, u64)>,
    oldest_kept: i64,
}

impl DemoSource {
    pub fn new() -> Self {
        Self {
            state: Arc::default(),
            changes: broadcast::channel(FEED_BUFFER).0,
        }
    }

    /// Serves until the listener fails; answers lookups and feeds on one port.
    pub async fn serve(self, listener: TcpListener) {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(self.clone().handle(stream));
        }
    }

    pub fn set(&self, discord_id: u64, value: Option<Value>) {
        let mut state = self.lock();
        match value {
            Some(value) => state.members.insert(discord_id, value),
            None => state.members.remove(&discord_id),
        };
        let seq = state.log.len() as i64 + 1;
        state.log.push((seq, discord_id));
        let _ = self.changes.send(change(seq, discord_id));
    }

    /// Forgets the change log, so a feed resuming from an older `seq` gets a reset.
    pub fn forget_history(&self) {
        let mut state = self.lock();
        state.oldest_kept = state.log.len() as i64 + 1;
    }

    async fn handle(self, mut stream: TcpStream) {
        let mut head = vec![0; HEAD_LIMIT];
        let Ok(read) = stream.peek(&mut head).await else {
            return;
        };
        let head = String::from_utf8_lossy(&head[..read]).to_ascii_lowercase();
        if head.contains("upgrade: websocket") {
            self.feed(stream).await;
        } else if let Some(path) = head.split_whitespace().nth(1).map(String::from) {
            let _ = stream.read(&mut vec![0; read]).await;
            let (status, body) = self.lookup(&path);
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    }

    fn lookup(&self, path: &str) -> (&'static str, String) {
        let member = path
            .strip_prefix("/members/")
            .and_then(|id| id.parse::<u64>().ok())
            .and_then(|id| self.lock().members.get(&id).cloned());
        match member {
            Some(value) => ("200 OK", value.to_string()),
            None => (
                "404 Not Found",
                json!({"error": "unknown member"}).to_string(),
            ),
        }
    }

    async fn feed(self, stream: TcpStream) {
        let mut after = 0;
        let accept = |request: &Request, response: Response| {
            after = request
                .uri()
                .query()
                .and_then(|query| query.strip_prefix("after="))
                .and_then(|seq| seq.parse().ok())
                .unwrap_or(0);
            Ok(response)
        };
        let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(stream, accept).await else {
            return;
        };
        let mut live = self.changes.subscribe();
        for message in self.replay(after) {
            if socket.send(Message::text(message)).await.is_err() {
                return;
            }
        }
        while let Ok(message) = live.recv().await {
            if socket.send(Message::text(message)).await.is_err() {
                return;
            }
        }
    }

    fn replay(&self, after: i64) -> Vec<String> {
        let state = self.lock();
        let mut messages = Vec::new();
        if after + 1 < state.oldest_kept {
            messages.push(json!({"reset": true}).to_string());
        }
        messages.extend(
            state
                .log
                .iter()
                .filter(|(seq, _)| *seq > after.max(state.oldest_kept - 1))
                .map(|&(seq, member)| change(seq, member)),
        );
        messages
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for DemoSource {
    fn default() -> Self {
        Self::new()
    }
}

fn change(seq: i64, discord_id: u64) -> String {
    json!({"seq": seq, "members": [discord_id.to_string()]}).to_string()
}
