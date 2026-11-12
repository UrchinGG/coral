//! Change feeds: `WebSocket {base}/changes?after=N`. A source replays everything
//! after `N`, then streams `{"seq": N, "members": ["<discord_id>", …]}`;
//! `{"reset": true}` means `N` is too old and every cached value is stale.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::task::AbortHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

use coral_redis::SyncEvent;
use database::{SyncSource, SyncSourceRepository};

use crate::framework::Data;

const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// The running followers, one per source with a feed.
#[derive(Clone, Default)]
pub struct Feeds(Arc<Mutex<HashMap<i64, Follower>>>);

struct Follower {
    task: AbortHandle,
    connected: bool,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct FeedStatus {
    pub connected: bool,
    pub error: Option<String>,
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
            connected: false,
            error: None,
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
        self.lock().get(&source_id).map(|follower| FeedStatus {
            connected: follower.connected,
            error: follower.error.clone(),
        })
    }

    fn report(&self, source_id: i64, connected: bool, error: Option<String>) {
        if let Some(follower) = self.lock().get_mut(&source_id) {
            follower.connected = connected;
            follower.error = error;
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
            .filter(|source| source.has_feed)
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
                data.feeds.report(source_id, false, Some(e.to_string()));
                tokio::time::sleep(backoff).await;
                continue;
            }
        };
        let error = match follow_once(&data, &source).await {
            Ok(()) => {
                backoff = MIN_BACKOFF;
                None
            }
            Err(e) => Some(e),
        };
        data.feeds.report(source_id, false, error);
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn follow_once(data: &Data, source: &SyncSource) -> Result<(), String> {
    let mut request = feed_url(&source.base_url, source.last_seq.unwrap_or(0))
        .into_client_request()
        .map_err(|e| e.to_string())?;
    if let Some(key) = &source.api_key {
        let bearer = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|e| e.to_string())?;
        request.headers_mut().insert("Authorization", bearer);
    }
    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| e.to_string())?;
    data.feeds.report(source.id, true, None);
    tracing::info!("Following the {} feed", source.name);
    while let Some(message) = socket.next().await {
        match message.map_err(|e| e.to_string())? {
            Message::Text(text) => apply(data, source, &text).await?,
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
    use serde_json::{Value, json};
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

    async fn register(data: &Data, base_url: &str, has_feed: bool) -> SyncSource {
        let name = format!("demo_{}", Utc::now().timestamp_nanos_opt().unwrap());
        SyncSourceRepository::new(data.db.pool())
            .add(&NewSyncSource {
                name: &name,
                base_url,
                api_key: None,
                has_feed,
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
            has_feed: false,
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
        let source = register(&data, &demo.base_url, false).await;
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
        let source = register(&data, &demo.base_url, true).await;
        let repo = SyncSourceRepository::new(data.db.pool());
        repo.store(source.id, 21, Some(&json!({"rank": "iron"})))
            .await
            .unwrap();
        repo.store(source.id, 22, Some(&json!({"rank": "iron"})))
            .await
            .unwrap();

        data.feeds.start(&data, source.id);
        eventually("the feed to connect", async || {
            data.feeds.status(source.id).is_some_and(|s| s.connected)
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

    #[test]
    fn a_source_value_reaches_the_nickname() {
        let mut context = json!({"displayname": "Nelly", "discord": {"name": "nelly"}});
        sources::add_external(&mut context, "demo", json!({"rank": "gold"}));
        sources::add_external(&mut context, "displayname", json!("clobbered"));
        let settings = Settings {
            nickname_template: Some("[{demo.rank}] {displayname}".into()),
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
        assert_eq!(context["displayname"], Value::from("Nelly"));
    }
}
