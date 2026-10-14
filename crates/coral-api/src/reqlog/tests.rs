use std::time::Duration;

use axum::Router;
use axum::http::{Method, Request as HttpRequest};
use chrono::{DateTime, SubsecRound};
use clients::{HypixelClient, MojangClient, RateBudget};
use coral_redis::RedisPool;
use database::{Database, permissions};
use reqlog::{Caller, Cause, KeyKind, LogEntry, LogReceiver, ROUTE_TEMPLATES};
use tower::ServiceExt;
use uuid::Uuid;

use crate::auth::{PERSONAL_RATE_LIMIT, SESSION_RATE_LIMIT};
use crate::state::{AppState, StarfishConfig};

const INTERNAL_KEY: &str = "internal-test-key";
const ENTRY_WAIT: Duration = Duration::from_secs(5);
const PLAYER: &str = "94589c87999b4a7ab3add2ae50711e64";
const OVERSIZED_BODY_BYTES: usize = 70 * 1024;
const TOO_LARGE_BODY_BYTES: usize = 3 * 1024 * 1024;
const UPSTREAM_TEST_KEY: &str = "reqlog-upstream-test-key";
const UPSTREAM_PENALTY_SECS: i64 = 60;

struct Harness {
    router: Router,
    log: LogReceiver,
    state: AppState,
}

struct Sent {
    status: u16,
    request_id_header: Option<String>,
    entry: LogEntry,
}

async fn harness() -> Option<Harness> {
    harness_with(|_| None).await
}

async fn harness_with(
    hypixel: impl FnOnce(&RedisPool) -> Option<HypixelClient>,
) -> Option<Harness> {
    dotenvy::dotenv().ok();
    let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
        .await
        .ok()?;
    let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
        .await
        .ok()?;
    let state = AppState::new(
        db,
        hypixel(&redis),
        MojangClient::new(redis.connection()),
        None,
        Some(INTERNAL_KEY.into()),
        redis,
        None,
        Some(starfish_config()),
        None,
        crate::cache::DEFAULT_BACKGROUND_REFRESH_BUDGET,
    );
    let (request_log, log) = reqlog::channel();
    Some(Harness {
        router: crate::build_router(state.clone(), request_log),
        log,
        state,
    })
}

fn starfish_config() -> StarfishConfig {
    StarfishConfig {
        hmac_secret: [3; 32],
        signing_key: ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        discord_client_id: "client".into(),
        discord_client_secret: "secret".into(),
        github_token: "token".into(),
        github_repo: "owner/repo".into(),
    }
}

impl Harness {
    async fn send(&mut self, request: HttpRequest<axum::body::Body>) -> Sent {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let request_id_header = response
            .headers()
            .get("x-request-id")
            .map(|value| value.to_str().unwrap().to_owned());
        let entry = tokio::time::timeout(ENTRY_WAIT, self.log.recv())
            .await
            .unwrap()
            .unwrap();
        Sent {
            status: response.status().as_u16(),
            request_id_header,
            entry,
        }
    }

    async fn get(&mut self, uri: &str, headers: &[(&str, &str)]) -> Sent {
        let mut request = HttpRequest::get(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        self.send(request.body(axum::body::Body::empty()).unwrap())
            .await
    }

    async fn member(&self, locked: bool) -> (i64, String) {
        let api_key = Uuid::new_v4().to_string();
        let discord_id = 870_000_000_000_000_000 + i64::from(Uuid::new_v4().as_fields().1);
        let id = sqlx::query_scalar(
            "INSERT INTO members (discord_id, api_key, key_locked) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(discord_id)
        .bind(&api_key)
        .bind(locked)
        .fetch_one(self.state.db.pool())
        .await
        .unwrap();
        (id, api_key)
    }

    async fn developer(&self, permissions: i64) -> (i64, String) {
        let (member_id, _) = self.member(false).await;
        let api_key = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO developer_keys (member_id, api_key, permissions) VALUES ($1, $2, $3)",
        )
        .bind(member_id)
        .bind(&api_key)
        .bind(permissions)
        .execute(self.state.db.pool())
        .await
        .unwrap();
        (member_id, api_key)
    }
}

fn concrete(template: &str) -> String {
    template
        .split('/')
        .map(|segment| match segment.starts_with('{') {
            true => "sample",
            false => segment,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[tokio::test]
async fn every_route_template_is_the_matched_path() {
    let Some(mut harness) = harness().await else {
        return;
    };
    for template in ROUTE_TEMPLATES {
        let request = HttpRequest::builder()
            .method(Method::TRACE)
            .uri(concrete(template))
            .body(axum::body::Body::empty())
            .unwrap();
        let sent = harness.send(request).await;
        assert_eq!(sent.entry.route.as_deref(), Some(*template), "{template}");
        assert_ne!(sent.status, 200, "{template}");
    }
}

#[tokio::test]
async fn unmatched_paths_have_no_route() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let sent = harness.get("/player/oJugg", &[]).await;
    assert_eq!(sent.status, 404);
    assert_eq!(sent.entry.route, None);
    assert_eq!(sent.entry.cause, Cause::Unmatched);
    assert_eq!(sent.entry.caller, Caller::anonymous(KeyKind::None));
}

#[tokio::test]
async fn the_request_id_is_returned_and_bound_to_the_timestamp() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let sent = harness.get("/v3/player/tags?player=oJugg", &[]).await;
    assert_eq!(
        sent.request_id_header,
        Some(sent.entry.request_id.to_string())
    );
    let (seconds, nanos) = sent.entry.request_id.get_timestamp().unwrap().to_unix();
    let embedded = DateTime::from_timestamp(seconds as i64, nanos).unwrap();
    assert_eq!(embedded, sent.entry.ts.trunc_subsecs(3));
}

#[tokio::test]
async fn missing_and_invalid_keys_are_attributed_on_401() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let sent = harness.get("/v3/player/tags?player=oJugg", &[]).await;
    assert_eq!(sent.status, 401);
    assert_eq!(sent.entry.caller, Caller::anonymous(KeyKind::None));
    assert_eq!(sent.entry.cause, Cause::Auth);

    let sent = harness
        .get(
            "/v3/player/tags?player=oJugg",
            &[("x-api-key", "deadbeef-0000-4000-8000-000000000000")],
        )
        .await;
    assert_eq!(sent.status, 401);
    assert_eq!(sent.entry.caller, Caller::anonymous(KeyKind::Invalid));
    assert_eq!(sent.entry.client.key_prefix.as_deref(), Some("deadbeef"));
}

#[tokio::test]
async fn locked_keys_are_attributed_on_403() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let (member_id, api_key) = harness.member(true).await;
    let sent = harness
        .get(
            &format!("/v3/player/tags?player={PLAYER}"),
            &[("x-api-key", &api_key)],
        )
        .await;
    assert_eq!(sent.status, 403);
    assert_eq!(
        sent.entry.caller,
        Caller::member(KeyKind::Personal, member_id)
    );
    assert_eq!(sent.entry.cause, Cause::Auth);
}

#[tokio::test]
async fn rate_limited_keys_are_attributed_on_429() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let (member_id, api_key) = harness.member(false).await;
    harness
        .state
        .rate_limiter
        .check_and_record_weighted(&api_key, PERSONAL_RATE_LIMIT, PERSONAL_RATE_LIMIT)
        .await
        .unwrap();
    let sent = harness
        .get(
            &format!("/v3/player/tags?player={PLAYER}"),
            &[("x-api-key", &api_key)],
        )
        .await;
    assert_eq!(sent.status, 429);
    assert_eq!(
        sent.entry.caller,
        Caller::member(KeyKind::Personal, member_id)
    );
    assert_eq!(sent.entry.cause, Cause::CallerLimit);
}

#[tokio::test]
async fn the_actor_header_is_trusted_only_with_the_internal_key() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let headers = [
        ("x-api-key", INTERNAL_KEY),
        ("x-coral-actor", "123456789012345678"),
        ("user-agent", "coral-bot"),
    ];
    let sent = harness
        .get(&format!("/v3/player/tags?player={PLAYER}"), &headers)
        .await;
    assert_eq!(sent.status, 200);
    assert_eq!(
        sent.entry.caller,
        Caller::internal(Some(123_456_789_012_345_678))
    );
    assert_eq!(sent.entry.service(), reqlog::Service::Bot);

    let (member_id, api_key) = harness.member(false).await;
    let sent = harness
        .get(
            &format!("/v3/player/tags?player={PLAYER}"),
            &[("x-api-key", &api_key), ("x-coral-actor", "1")],
        )
        .await;
    assert_eq!(sent.status, 200);
    assert_eq!(
        sent.entry.caller,
        Caller::member(KeyKind::Personal, member_id)
    );
}

#[tokio::test]
async fn cubelify_rejections_record_their_semantic_outcome() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let sent = harness
        .get(&format!("/v3/cubelify?uuid={PLAYER}&key=nope"), &[])
        .await;
    assert_eq!(sent.status, 200);
    assert_eq!(sent.entry.outcome_status, Some(401));
    assert_eq!(sent.entry.semantic_status(), 401);
    assert_eq!(sent.entry.cause, Cause::Auth);
    assert_eq!(sent.entry.error.as_deref(), Some("invalid_key"));
    assert_eq!(sent.entry.caller, Caller::anonymous(KeyKind::Invalid));

    let (member_id, api_key) = harness.member(false).await;
    let sent = harness
        .get(&format!("/v3/cubelify?uuid={PLAYER}&key={api_key}"), &[])
        .await;
    assert_eq!(sent.status, 200);
    assert_eq!(sent.entry.outcome_status, None);
    assert_eq!(sent.entry.cause, Cause::Ok);
    assert_eq!(
        sent.entry.caller,
        Caller::member(KeyKind::Personal, member_id)
    );
}

#[tokio::test]
async fn rejected_bodies_keep_a_labelled_prefix() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let body = format!(r#"{{"uuids":["{}"]}}"#, "a".repeat(OVERSIZED_BODY_BYTES));
    let request = HttpRequest::post("/v3/players")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.clone()))
        .unwrap();
    let sent = harness.send(request).await;
    assert_eq!(sent.status, 401);
    let detail = &sent.entry.detail;
    assert_eq!(detail.body_bytes as usize, body.len());
    assert!(!detail.body_complete);
    assert_eq!(
        detail.body.as_deref().map(str::len),
        Some(reqlog::STORED_BODY_BYTES)
    );

    let request = HttpRequest::post("/v3/players")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(r#"{"uuids":[]}"#))
        .unwrap();
    let sent = harness.send(request).await;
    assert!(!sent.entry.detail.body_complete);
    assert_eq!(sent.entry.detail.body.as_deref(), Some(r#"{"uuids":[]}"#));
}

#[tokio::test]
async fn oversized_bodies_record_their_full_size_and_a_prefix() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let request = HttpRequest::post("/v3/players")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(vec![b'a'; TOO_LARGE_BODY_BYTES]))
        .unwrap();
    let sent = harness.send(request).await;
    assert_eq!(sent.status, 413);
    let detail = &sent.entry.detail;
    assert_eq!(detail.body_bytes as usize, TOO_LARGE_BODY_BYTES);
    assert!(!detail.body_complete);
    assert_eq!(
        detail.body.as_deref().map(str::len),
        Some(reqlog::STORED_BODY_BYTES)
    );
}

#[tokio::test]
async fn nul_bytes_in_an_accepted_body_are_replaced() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let request = HttpRequest::get(format!("/v3/player/tags?player={PLAYER}"))
        .header("x-api-key", INTERNAL_KEY)
        .body(axum::body::Body::from(&b"a\0b"[..]))
        .unwrap();
    let sent = harness.send(request).await;
    assert_eq!(sent.status, 200);
    let detail = &sent.entry.detail;
    assert_eq!(detail.body.as_deref(), Some("a\u{FFFD}b"));
    assert_eq!(detail.body_bytes, 3);
    assert!(!detail.body_complete);
}

struct StarfishSession {
    discord_id: i64,
    headers: Vec<(&'static str, String)>,
}

impl Harness {
    async fn starfish_session(&self) -> StarfishSession {
        let pool = self.state.db.pool();
        let discord_id = 860_000_000_000_000_000 + i64::from(Uuid::new_v4().as_fields().1);
        let hwid = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let token = Uuid::new_v4().simple().to_string();
        let signature = Uuid::new_v4().as_bytes().to_vec();
        let user_id: i64 = sqlx::query_scalar(
            "INSERT INTO starfish_users (discord_id, license_status) VALUES ($1, 'active') RETURNING id",
        )
        .bind(discord_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let hwid_id: i64 = sqlx::query_scalar(
            "INSERT INTO starfish_hwids (user_id, hwid_hash) VALUES ($1, $2) RETURNING id",
        )
        .bind(user_id)
        .bind(&hwid)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO starfish_sessions (user_id, hwid_id, session_token, core_data, expires_at, signature)
             VALUES ($1, $2, $3, '\\x00', now() + interval '1 hour', $4)",
        )
        .bind(user_id)
        .bind(hwid_id)
        .bind(&token)
        .bind(&signature)
        .execute(pool)
        .await
        .unwrap();
        StarfishSession {
            discord_id,
            headers: vec![
                ("x-starfish-session", token),
                ("x-starfish-hwid", hwid),
                ("x-starfish-signature", hex::encode(signature)),
            ],
        }
    }
}

impl StarfishSession {
    fn headers(&self) -> Vec<(&str, &str)> {
        self.headers
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect()
    }

    fn with_signature(&self, signature: &'static str) -> Vec<(&str, &str)> {
        self.headers
            .iter()
            .map(|(name, value)| match *name {
                "x-starfish-signature" => (*name, signature),
                _ => (*name, value.as_str()),
            })
            .collect()
    }
}

#[tokio::test]
async fn starfish_sessions_are_attributed_to_their_discord_user() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let session = harness.starfish_session().await;
    let sent = harness
        .get("/api/v1/starfish/users/me", &session.headers())
        .await;
    assert_eq!(sent.status, 200);
    assert_eq!(sent.entry.caller, Caller::session(None, session.discord_id));

    let sent = harness
        .get("/api/v1/starfish/users/me", &session.with_signature("00"))
        .await;
    assert_eq!(sent.status, 401);
    assert_eq!(sent.entry.caller, Caller::session(None, session.discord_id));

    let sent = harness
        .get(
            &format!("/v3/player/tags?player={PLAYER}"),
            &session.headers(),
        )
        .await;
    assert_eq!(sent.status, 200);
    let member_id: i64 = sqlx::query_scalar("SELECT id FROM members WHERE discord_id = $1")
        .bind(session.discord_id)
        .fetch_one(harness.state.db.pool())
        .await
        .unwrap();
    assert_eq!(
        sent.entry.caller,
        Caller::session(Some(member_id), session.discord_id)
    );
}

#[tokio::test]
async fn moderator_routes_attribute_rejected_personal_keys() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let (member_id, api_key) = harness.member(false).await;
    let request = HttpRequest::post(format!("/v3/player/lock?player={PLAYER}"))
        .header("x-api-key", &api_key)
        .body(axum::body::Body::empty())
        .unwrap();
    let sent = harness.send(request).await;
    assert_eq!(sent.status, 403);
    assert_eq!(
        sent.entry.caller,
        Caller::member(KeyKind::Personal, member_id)
    );
    assert_eq!(sent.entry.cause, Cause::Auth);
}

#[tokio::test]
async fn moderator_routes_attribute_unknown_keys_as_invalid() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let request = HttpRequest::post(format!("/v3/player/lock?player={PLAYER}"))
        .header("x-api-key", "deadbeef-nomatch")
        .body(axum::body::Body::empty())
        .unwrap();
    let sent = harness.send(request).await;
    assert_eq!(sent.status, 401);
    assert_eq!(sent.entry.caller, Caller::anonymous(KeyKind::Invalid));
    assert_eq!(sent.entry.client.key_prefix.as_deref(), Some("deadbeef"));
}

#[tokio::test]
async fn rate_limited_sessions_keep_their_member() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let session = harness.starfish_session().await;
    let member_id: i64 =
        sqlx::query_scalar("INSERT INTO members (discord_id) VALUES ($1) RETURNING id")
            .bind(session.discord_id)
            .fetch_one(harness.state.db.pool())
            .await
            .unwrap();
    harness
        .state
        .rate_limiter
        .check_and_record_weighted(
            &format!("sf:{}", session.discord_id),
            SESSION_RATE_LIMIT,
            SESSION_RATE_LIMIT,
        )
        .await
        .unwrap();
    let sent = harness
        .get(
            &format!("/v3/player/tags?player={PLAYER}"),
            &session.headers(),
        )
        .await;
    assert_eq!(sent.status, 429);
    assert_eq!(
        sent.entry.caller,
        Caller::session(Some(member_id), session.discord_id)
    );
    assert_eq!(sent.entry.cause, Cause::CallerLimit);
}

#[tokio::test]
async fn upstream_limits_reach_the_log_with_their_caller() {
    let Some(mut harness) = harness_with(|redis| {
        Some(HypixelClient::new(UPSTREAM_TEST_KEY.into(), redis.connection()).unwrap())
    })
    .await
    else {
        return;
    };
    RateBudget::new(harness.state.redis.connection())
        .penalize(UPSTREAM_TEST_KEY, UPSTREAM_PENALTY_SECS)
        .await;
    let (member_id, api_key) = harness.developer(permissions::HYPIXEL).await;
    let player = Uuid::new_v4().simple().to_string();
    let sent = harness
        .get(
            &format!("/v3/hypixel/player?player={player}"),
            &[("x-api-key", &api_key)],
        )
        .await;
    assert_eq!(sent.status, 429);
    assert_eq!(sent.entry.cause, Cause::UpstreamLimit);
    assert_eq!(
        sent.entry.caller,
        Caller::member(KeyKind::Developer, member_id)
    );
    assert_eq!(
        sent.entry.error.as_deref(),
        Some(r#"{"error":"Rate limited"}"#)
    );
}
