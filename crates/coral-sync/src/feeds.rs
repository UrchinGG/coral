//! Change feeds: `WebSocket {base}/changes?after=N`. A source replays everything
//! after `N`, then streams `{"seq": N, "members": ["<discord_id>", …]}`;
//! `{"reset": true}` means `N` is too old and every cached value is stale.
//!
//! Every source is tried; one whose upgrade answers 404 has no feed and is pulled
//! instead. While a feed is down its cached values fall back to the maximum age.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::task::AbortHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::{Error, Message};

use coral_redis::SyncEvent;
use database::{SyncSource, SyncSourceRepository};

use crate::framework::Data;

const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// How often a source without a feed is asked again, in case it gains one.
const NO_FEED_RETRY: Duration = Duration::from_secs(3600);

/// The running followers, one per source with a feed.
#[derive(Clone, Default)]
pub struct Feeds(Arc<Mutex<HashMap<i64, Follower>>>);

struct Follower {
    task: AbortHandle,
    status: FeedStatus,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedState {
    Connecting,
    Connected,
    /// The source answered 404 to the upgrade: it is pulled, not pushed.
    None,
    Disconnected,
}

#[derive(Clone, Serialize)]
pub struct FeedStatus {
    pub state: FeedState,
    pub error: Option<String>,
}

enum FeedError {
    NoFeed,
    Failed(String),
}

#[derive(Deserialize)]
struct FeedMessage {
    #[serde(default)]
    seq: Option<i64>,
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    reset: bool,
}

impl Feeds {
    pub fn start(&self, data: &Data, source_id: i64) {
        let task = tokio::spawn(follow(data.clone(), source_id)).abort_handle();
        let follower = Follower {
            task,
            status: FeedStatus {
                state: FeedState::Connecting,
                error: None,
            },
        };
        if let Some(old) = self.lock().insert(source_id, follower) {
            old.task.abort();
        }
    }

    pub fn stop(&self, source_id: i64) {
        if let Some(follower) = self.lock().remove(&source_id) {
            follower.task.abort();
        }
    }

    pub fn status(&self, source_id: i64) -> Option<FeedStatus> {
        self.lock()
            .get(&source_id)
            .map(|follower| follower.status.clone())
    }

    pub fn is_connected(&self, source_id: i64) -> bool {
        self.status(source_id)
            .is_some_and(|status| status.state == FeedState::Connected)
    }

    fn report(&self, source_id: i64, state: FeedState, error: Option<String>) {
        if let Some(follower) = self.lock().get_mut(&source_id) {
            follower.status = FeedStatus { state, error };
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<i64, Follower>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub async fn start_all(data: &Data) {
    match SyncSourceRepository::new(data.db.pool()).list().await {
        Ok(sources) => sources
            .iter()
            .for_each(|source| data.feeds.start(data, source.id)),
        Err(e) => tracing::error!("Feeds not started: read the sources: {e}"),
    }
}

async fn follow(data: Data, source_id: i64) {
    let mut backoff = MIN_BACKOFF;
    loop {
        let source = match SyncSourceRepository::new(data.db.pool())
            .get(source_id)
            .await
        {
            Ok(Some(source)) => source,
            Ok(None) => return,
            Err(e) => {
                data.feeds
                    .report(source_id, FeedState::Disconnected, Some(e.to_string()));
                tokio::time::sleep(backoff).await;
                continue;
            }
        };
        match follow_once(&data, &source).await {
            Ok(()) => {
                backoff = MIN_BACKOFF;
                data.feeds.report(source_id, FeedState::Disconnected, None);
            }
            Err(FeedError::NoFeed) => {
                data.feeds.report(source_id, FeedState::None, None);
                tokio::time::sleep(NO_FEED_RETRY).await;
                continue;
            }
            Err(FeedError::Failed(e)) => {
                data.feeds
                    .report(source_id, FeedState::Disconnected, Some(e));
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn follow_once(data: &Data, source: &SyncSource) -> Result<(), FeedError> {
    let failed = |e: &dyn std::fmt::Display| FeedError::Failed(e.to_string());
    let mut request = feed_url(&source.base_url, source.last_seq.unwrap_or(0))
        .into_client_request()
        .map_err(|e| failed(&e))?;
    if let Some(key) = &source.api_key {
        let bearer = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|e| failed(&e))?;
        request.headers_mut().insert("Authorization", bearer);
    }
    let (mut socket, _) = match tokio_tungstenite::connect_async(request).await {
        Ok(connected) => connected,
        Err(Error::Http(response)) if response.status() == StatusCode::NOT_FOUND => {
            return Err(FeedError::NoFeed);
        }
        Err(e) => return Err(failed(&e)),
    };
    data.feeds.report(source.id, FeedState::Connected, None);
    tracing::info!("Following the {} feed", source.name);
    while let Some(message) = socket.next().await {
        match message.map_err(|e| failed(&e))? {
            Message::Text(text) => apply(data, source, &text).await.map_err(|e| failed(&e))?,
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

async fn apply(data: &Data, source: &SyncSource, text: &str) -> Result<(), String> {
    let message: FeedMessage = serde_json::from_str(text)
        .map_err(|e| format!("{} sent an unreadable feed message: {e}", source.name))?;
    let repo = SyncSourceRepository::new(data.db.pool());
    if message.reset {
        tracing::info!("The {} feed was reset; its cache is stale", source.name);
        repo.mark_all_stale(source.id)
            .await
            .map_err(|e| e.to_string())?;
    }
    // shortcut: members repeated across messages sync once per message, not once per burst; coalesce per server if bursts get large.
    let members: BTreeSet<u64> = message
        .members
        .iter()
        .filter_map(|id| id.parse().ok())
        .collect();
    let ids: Vec<i64> = members.iter().map(|&id| id as i64).collect();
    repo.mark_stale(source.id, &ids)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(seq) = message.seq {
        repo.set_last_seq(source.id, seq)
            .await
            .map_err(|e| e.to_string())?;
    }
    for discord_id in members {
        crate::events::publish(data, SyncEvent::SyncUser { discord_id }).await;
    }
    Ok(())
}

fn feed_url(base_url: &str, after: i64) -> String {
    let base = base_url.trim_end_matches('/');
    let socket_base = base
        .strip_prefix("https://")
        .map(|rest| format!("wss://{rest}"))
        .or_else(|| {
            base.strip_prefix("http://")
                .map(|rest| format!("ws://{rest}"))
        })
        .unwrap_or_else(|| base.to_string());
    format!("{socket_base}/changes?after={after}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_urls_switch_to_websocket_schemes() {
        assert_eq!(
            feed_url("https://rbw.urchin.gg/coral/v1/", 7),
            "wss://rbw.urchin.gg/coral/v1/changes?after=7"
        );
        assert_eq!(
            feed_url("http://demo:9000", 0),
            "ws://demo:9000/changes?after=0"
        );
    }

    #[test]
    fn feed_messages_default_their_missing_fields() {
        let reset: FeedMessage = serde_json::from_str(r#"{"reset": true}"#).unwrap();
        assert!(reset.reset && reset.members.is_empty() && reset.seq.is_none());
        let change: FeedMessage = serde_json::from_str(r#"{"seq": 4, "members": ["12"]}"#).unwrap();
        assert_eq!(
            (change.seq, change.members.len(), change.reset),
            (Some(4), 1, false)
        );
    }
}

#[cfg(test)]
mod demo_tests {
    use std::time::Instant;

    use chrono::Utc;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    use database::NewSyncSource;

    use super::*;
    use crate::demo_source::DemoSource;
    use crate::engine::{self, Current, Settings};
    use crate::sources::{self, Unavailable};

    const WAIT: Duration = Duration::from_secs(5);

    struct Demo {
        source: DemoSource,
        server: JoinHandle<()>,
        base_url: String,
    }

    async fn demo() -> Demo {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let source = DemoSource::new();
        let server = tokio::spawn(source.clone().serve(listener));
        Demo {
            source,
            server,
            base_url,
        }
    }

    async fn register(data: &Data, base_url: &str) -> SyncSource {
        let name = format!("demo_{}", Utc::now().timestamp_nanos_opt().unwrap());
        SyncSourceRepository::new(data.db.pool())
            .add(&NewSyncSource {
                name: &name,
                base_url,
                api_key: None,
                max_age_secs: 3600,
                created_by: 1,
            })
            .await
            .unwrap()
            .unwrap()
    }

    async fn eventually(what: &str, check: impl AsyncFn() -> bool) {
        let started = Instant::now();
        while !check().await {
            assert!(started.elapsed() < WAIT, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn lookups_distinguish_known_unknown_and_down() {
        let demo = demo().await;
        let source = SyncSource {
            id: 0,
            name: "demo".into(),
            base_url: demo.base_url.clone(),
            api_key: None,
            max_age_secs: 1,
            last_seq: None,
            created_at: Utc::now(),
        };
        demo.source.set(7, Some(json!({"rank": "gold"})));
        assert_eq!(
            sources::lookup(&source, 7).await.unwrap(),
            Some(json!({"rank": "gold"}))
        );
        assert_eq!(sources::lookup(&source, 8).await.unwrap(), None);
        demo.server.abort();
        let _ = demo.server.await;
        assert!(sources::lookup(&source, 7).await.is_err());
    }

    #[tokio::test]
    async fn a_down_source_serves_its_cache_and_never_guesses() {
        let Some(data) = crate::testing::test_data().await else {
            return;
        };
        let demo = demo().await;
        let source = register(&data, &demo.base_url).await;
        demo.source.set(11, Some(json!({"rank": "gold"})));

        let gold = Some(json!({"rank": "gold"}));
        assert_eq!(sources::external(&data, &source, 11).await.unwrap(), gold);
        demo.server.abort();
        let _ = demo.server.await;
        SyncSourceRepository::new(data.db.pool())
            .mark_all_stale(source.id)
            .await
            .unwrap();

        assert_eq!(sources::external(&data, &source, 11).await.unwrap(), gold);
        assert!(matches!(
            sources::external(&data, &source, 12).await,
            Err(Unavailable)
        ));
    }

    #[tokio::test]
    async fn a_pushed_change_marks_the_member_stale_and_a_reset_marks_everyone() {
        let Some(data) = crate::testing::test_data().await else {
            return;
        };
        let demo = demo().await;
        let source = register(&data, &demo.base_url).await;
        let repo = SyncSourceRepository::new(data.db.pool());
        repo.store(source.id, 21, Some(&json!({"rank": "iron"})))
            .await
            .unwrap();
        repo.store(source.id, 22, Some(&json!({"rank": "iron"})))
            .await
            .unwrap();

        data.feeds.start(&data, source.id);
        eventually("the feed to connect", async || {
            data.feeds.is_connected(source.id)
        })
        .await;
        demo.source.set(21, Some(json!({"rank": "gold"})));
        eventually("member 21 to go stale", async || {
            repo.cached(source.id, 21).await.unwrap().unwrap().stale
        })
        .await;
        assert!(!repo.cached(source.id, 22).await.unwrap().unwrap().stale);
        let refreshed = sources::external(&data, &source, 21).await.unwrap();
        assert_eq!(refreshed, Some(json!({"rank": "gold"})));
        eventually("the seq to be stored", async || {
            repo.get(source.id).await.unwrap().unwrap().last_seq == Some(1)
        })
        .await;

        data.feeds.stop(source.id);
        demo.source.forget_history();
        repo.set_last_seq(source.id, 0).await.unwrap();
        data.feeds.start(&data, source.id);
        eventually("the reset to mark member 22 stale", async || {
            repo.cached(source.id, 22).await.unwrap().unwrap().stale
        })
        .await;
        data.feeds.stop(source.id);
    }

    #[tokio::test]
    async fn a_source_that_refuses_the_upgrade_has_no_feed() {
        let Some(data) = crate::testing::test_data().await else {
            return;
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut request = [0; 4096];
                let _ = stream.read(&mut request).await;
                let not_found =
                    b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
                let _ = stream.write_all(not_found).await;
            }
        });
        let source = register(&data, &base_url).await;

        data.feeds.start(&data, source.id);
        eventually("the source to be marked feedless", async || {
            data.feeds
                .status(source.id)
                .is_some_and(|status| status.state == FeedState::None)
        })
        .await;
        assert!(!data.feeds.is_connected(source.id));
        data.feeds.stop(source.id);
    }

    #[test]
    fn a_source_value_reaches_the_nickname() {
        let mut context =
            json!({"hypixel": {"displayname": "Nelly"}, "discord": {"name": "nelly"}});
        sources::add_external(&mut context, "demo", json!({"rank": "gold"}));
        let settings = Settings {
            nickname_template: Some("[{demo.rank}] {hypixel.displayname}".into()),
            link_role: None,
            unlinked_role: None,
            rules: Vec::new(),
        };
        let current = Current {
            roles: &[],
            nick: None,
        };
        let changes = engine::linked_changes(&settings, &context, &current, false, false);
        assert_eq!(changes.nickname.as_deref(), Some("[gold] Nelly"));
    }
}
