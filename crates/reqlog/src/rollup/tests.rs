use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::KEY_PREFIX_CHARS;
use crate::entry::{Cause, KeyKind, Service};
use crate::routes::template_for_path;
use crate::test_support::{NETWORK_STATE, create_day_partitions, test_pool};

const ROW_DAY: (i32, u32, u32) = (2020, 3, 1);
const VOLUME_DAY: (i32, u32, u32) = (2020, 3, 2);
const NETWORK_DAY: (i32, u32, u32) = (2020, 3, 3);
const CARDINALITY_DAY: (i32, u32, u32) = (2020, 3, 4);
const VOLUME_ROWS: i32 = 3_000;
const VOLUME_HOURS: i64 = 3;
const DISTINCT_CALLER_GROUPS: i32 = 20_000;
const HIGH_CARDINALITY_BOUND: Duration = Duration::from_secs(10);
const UPSTREAM_LIMITED: &str = r#"{"error":"Rate limited"}"#;

type CallerKey = (
    i16,
    Option<i64>,
    Option<i64>,
    Option<String>,
    i16,
    Option<String>,
    String,
    i16,
    i16,
);

#[derive(Clone)]
struct Seed {
    minute: u32,
    request_id: Option<Uuid>,
    method: &'static str,
    path: &'static str,
    route: Option<&'static str>,
    status: i16,
    outcome_status: Option<i16>,
    cause: Option<Cause>,
    key_kind: &'static str,
    member_id: Option<i64>,
    actor_id: Option<i64>,
    key_prefix: Option<String>,
    user_agent: Option<&'static str>,
    error: Option<&'static str>,
    subjects: Option<i16>,
}

struct Expected {
    key_kind: KeyKind,
    member_id: Option<i64>,
    key_prefix: Option<String>,
    service: Service,
    route: Option<&'static str>,
    status: i16,
    cause: Cause,
}

struct Keys {
    member: i64,
    retired_owner: i64,
    personal: String,
    developer: String,
    retired: String,
    shared: String,
    internal: String,
    unknown: String,
    members: Vec<i64>,
}

#[derive(FromRow)]
struct CallerRow {
    key_kind: i16,
    member_id: Option<i64>,
    actor_id: Option<i64>,
    key_prefix: Option<String>,
    service: i16,
    route: Option<String>,
    method: String,
    status: i16,
    cause: i16,
    requests: i32,
}

struct NetworkRow {
    minute: i64,
    member_id: Option<i64>,
    key_kind: &'static str,
    actor_id: Option<i64>,
    ip: &'static str,
    country: Option<&'static str>,
    user_agent: &'static str,
    request_id: Option<Uuid>,
}

fn day((year, month, day): (i32, u32, u32)) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn on(date: (i32, u32, u32), hour: u32, minute: u32) -> DateTime<Utc> {
    let (year, month, day) = date;
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

fn random_prefix() -> String {
    Uuid::new_v4().simple().to_string()[..KEY_PREFIX_CHARS].to_string()
}

fn random_key(prefix: &str) -> String {
    format!("{prefix}{}", Uuid::new_v4().simple())
}

async fn fixture(date: (i32, u32, u32)) -> Option<PgPool> {
    let pool = test_pool().await?;
    create_day_partitions(&pool, day(date)).await;
    Some(pool)
}

async fn clear(pool: &PgPool, span: Span) {
    for table in ["api_request_log", "api_request_detail"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE ts >= $1 AND ts < $2"))
            .bind(span.from)
            .bind(span.to)
            .execute(pool)
            .await
            .unwrap();
    }
    for table in ROLLUP_TABLES {
        sqlx::query(&format!(
            "DELETE FROM {name} WHERE {time} >= $1 AND {time} < $2",
            name = table.name,
            time = table.time
        ))
        .bind(span.from)
        .bind(span.to)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn new_member(pool: &PgPool) -> (i64, i64) {
    let discord_id = 880_200_000_000_000_000 + (Uuid::new_v4().as_u128() % 1_000_000_000) as i64;
    let id = sqlx::query_scalar("INSERT INTO members (discord_id) VALUES ($1) RETURNING id")
        .bind(discord_id)
        .fetch_one(pool)
        .await
        .unwrap();
    (id, discord_id)
}

async fn remove_members(pool: &PgPool, ids: &[i64]) {
    sqlx::query("DELETE FROM members WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await
        .unwrap();
}

async fn add_key(pool: &PgPool, member_id: i64, kind: &str, key: &str, retired: bool) {
    sqlx::query(
        "INSERT INTO member_api_keys (member_id, kind, api_key, retired_at)
         VALUES ($1, $2, $3, CASE WHEN $4 THEN now() END)",
    )
    .bind(member_id)
    .bind(kind)
    .bind(key)
    .bind(retired)
    .execute(pool)
    .await
    .unwrap();
}

async fn keys(pool: &PgPool) -> Keys {
    let (member, _) = new_member(pool).await;
    let (retired_owner, _) = new_member(pool).await;
    let (other, _) = new_member(pool).await;
    let keys = Keys {
        member,
        retired_owner,
        personal: random_prefix(),
        developer: random_prefix(),
        retired: random_prefix(),
        shared: random_prefix(),
        internal: random_prefix(),
        unknown: random_prefix(),
        members: vec![member, retired_owner, other],
    };
    add_key(pool, member, "personal", &random_key(&keys.personal), false).await;
    add_key(
        pool,
        member,
        "developer",
        &random_key(&keys.developer),
        false,
    )
    .await;
    add_key(
        pool,
        retired_owner,
        "personal",
        &random_key(&keys.retired),
        true,
    )
    .await;
    add_key(
        pool,
        retired_owner,
        "personal",
        &random_key(&keys.shared),
        false,
    )
    .await;
    add_key(pool, other, "personal", &random_key(&keys.shared), false).await;
    keys
}

async fn insert(pool: &PgPool, date: (i32, u32, u32), seed: &Seed) {
    sqlx::query(
        "INSERT INTO api_request_log (ts, request_id, method, path, route, status, outcome_status,
           cause, key_kind, member_id, actor_id, key_prefix, user_agent, error, subjects, latency_ms)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 7)",
    )
    .bind(on(date, 10, seed.minute))
    .bind(seed.request_id)
    .bind(seed.method)
    .bind(seed.path)
    .bind(seed.route)
    .bind(seed.status)
    .bind(seed.outcome_status)
    .bind(seed.cause.map(Cause::code))
    .bind(seed.key_kind)
    .bind(seed.member_id)
    .bind(seed.actor_id)
    .bind(&seed.key_prefix)
    .bind(seed.user_agent)
    .bind(seed.error)
    .bind(seed.subjects)
    .execute(pool)
    .await
    .unwrap();
}

fn legacy(minute: u32, path: &'static str, status: i16) -> Seed {
    Seed {
        minute,
        request_id: None,
        method: "GET",
        path,
        route: None,
        status,
        outcome_status: None,
        cause: None,
        key_kind: "key",
        member_id: None,
        actor_id: None,
        key_prefix: None,
        user_agent: None,
        error: None,
        subjects: None,
    }
}

fn recorded(minute: u32, route: &'static str, key_kind: KeyKind, cause: Cause) -> Seed {
    Seed {
        request_id: Some(crate::request_id_for(on(ROW_DAY, 10, minute))),
        route: Some(route),
        cause: Some(cause),
        key_kind: key_kind.label(),
        subjects: Some(0),
        ..legacy(minute, route, 200)
    }
}

fn expected(
    key_kind: KeyKind,
    member_id: Option<i64>,
    route: Option<&'static str>,
    status: i16,
    cause: Cause,
) -> Expected {
    Expected {
        key_kind,
        member_id,
        key_prefix: None,
        service: Service::Unknown,
        route,
        status,
        cause,
    }
}

fn cases(keys: &Keys) -> Vec<(Seed, Expected)> {
    let tags = Some("/v3/player/tags");
    vec![
        (
            Seed {
                member_id: Some(keys.member),
                key_prefix: Some(keys.personal.clone()),
                ..legacy(1, "/v3/player/tags", 200)
            },
            expected(KeyKind::Personal, Some(keys.member), tags, 200, Cause::Ok),
        ),
        (
            Seed {
                key_prefix: Some(keys.personal.clone()),
                ..legacy(2, "/v3/player/tags", 429)
            },
            expected(
                KeyKind::Personal,
                Some(keys.member),
                tags,
                429,
                Cause::CallerLimit,
            ),
        ),
        (
            Seed {
                method: "POST",
                key_prefix: Some(keys.developer.clone()),
                error: Some(UPSTREAM_LIMITED),
                ..legacy(3, "/v3/players", 429)
            },
            expected(
                KeyKind::Developer,
                Some(keys.member),
                Some("/v3/players"),
                429,
                Cause::UpstreamLimit,
            ),
        ),
        (
            Seed {
                key_prefix: Some(keys.unknown.clone()),
                ..legacy(4, "/v3/resolve/oJugg", 401)
            },
            expected(
                KeyKind::Invalid,
                None,
                Some("/v3/resolve/{identifier}"),
                401,
                Cause::Auth,
            ),
        ),
        (
            Seed {
                key_prefix: Some(keys.internal.clone()),
                user_agent: Some("coral-bot/1.0"),
                ..legacy(5, "/v3/hypixel/player", 200)
            },
            Expected {
                service: Service::Bot,
                ..expected(
                    KeyKind::Internal,
                    None,
                    Some("/v3/hypixel/player"),
                    200,
                    Cause::Ok,
                )
            },
        ),
        (
            Seed {
                key_kind: "none",
                member_id: Some(keys.member),
                error: Some(UPSTREAM_LIMITED),
                ..legacy(6, "/api/v1/starfish/users/me", 429)
            },
            expected(
                KeyKind::Session,
                Some(keys.member),
                Some("/api/v1/starfish/users/me"),
                429,
                Cause::CallerLimit,
            ),
        ),
        (
            Seed {
                key_kind: "none",
                ..legacy(7, "/wp-login.php", 404)
            },
            expected(KeyKind::None, None, None, 404, Cause::Unmatched),
        ),
        (
            Seed {
                member_id: Some(keys.retired_owner),
                key_prefix: Some(random_prefix()),
                ..legacy(8, "/v3/player/tags", 200)
            },
            expected(
                KeyKind::Legacy,
                Some(keys.retired_owner),
                tags,
                200,
                Cause::Ok,
            ),
        ),
        (
            Seed {
                key_prefix: Some(keys.shared.clone()),
                ..legacy(9, "/v3/player/tags", 403)
            },
            expected(KeyKind::Invalid, None, tags, 403, Cause::Auth),
        ),
        (
            Seed {
                request_id: Some(Uuid::new_v4()),
                key_prefix: Some(keys.retired.clone()),
                ..legacy(10, "/v3/player/tags", 401)
            },
            expected(
                KeyKind::Personal,
                Some(keys.retired_owner),
                tags,
                401,
                Cause::Auth,
            ),
        ),
        (
            legacy(11, "/v3/resolve/someone", 404),
            expected(
                KeyKind::Invalid,
                None,
                Some("/v3/resolve/{identifier}"),
                404,
                Cause::Validation,
            ),
        ),
        (
            Seed {
                key_kind: "none",
                ..legacy(12, "/v3/player/tags", 500)
            },
            expected(KeyKind::None, None, tags, 500, Cause::Internal),
        ),
        (
            Seed {
                member_id: Some(keys.member),
                subjects: Some(2),
                ..recorded(13, "/v3/player/tags", KeyKind::Personal, Cause::Ok)
            },
            expected(KeyKind::Personal, Some(keys.member), tags, 200, Cause::Ok),
        ),
        (
            Seed {
                key_prefix: Some(keys.retired.clone()),
                status: 401,
                ..recorded(14, "/v3/player/tags", KeyKind::Invalid, Cause::Auth)
            },
            Expected {
                key_prefix: Some(keys.retired.clone()),
                ..expected(KeyKind::Invalid, None, tags, 401, Cause::Auth)
            },
        ),
        (
            Seed {
                key_prefix: Some(keys.unknown.clone()),
                status: 401,
                ..recorded(15, "/v3/player/tags", KeyKind::Invalid, Cause::Auth)
            },
            expected(KeyKind::Invalid, None, tags, 401, Cause::Auth),
        ),
        (
            Seed {
                actor_id: Some(42),
                user_agent: Some("coral-sync/2"),
                ..recorded(16, "/v3/player/tags", KeyKind::Internal, Cause::Ok)
            },
            Expected {
                service: Service::Sync,
                ..expected(KeyKind::Internal, None, tags, 200, Cause::Ok)
            },
        ),
        (
            Seed {
                member_id: Some(keys.member),
                outcome_status: Some(429),
                ..recorded(17, "/v3/cubelify", KeyKind::Personal, Cause::CallerLimit)
            },
            expected(
                KeyKind::Personal,
                Some(keys.member),
                Some("/v3/cubelify"),
                429,
                Cause::CallerLimit,
            ),
        ),
        (
            Seed {
                key_kind: "mystery",
                ..recorded(18, "/v3/player/tags", KeyKind::None, Cause::Ok)
            },
            expected(KeyKind::Legacy, None, tags, 200, Cause::Ok),
        ),
    ]
}

fn caller_key(seed: &Seed, expected: &Expected) -> CallerKey {
    (
        expected.key_kind.code(),
        expected.member_id,
        seed.actor_id,
        expected.key_prefix.clone(),
        expected.service.code(),
        expected.route.map(str::to_owned),
        seed.method.to_owned(),
        expected.status,
        expected.cause.code(),
    )
}

async fn caller_rows(pool: &PgPool, hour: DateTime<Utc>) -> HashMap<CallerKey, i32> {
    let rows: Vec<CallerRow> = sqlx::query_as(
        "SELECT key_kind, member_id, actor_id, key_prefix, service, route, method, status, cause,
                requests
         FROM api_caller_hour WHERE hour = $1",
    )
    .bind(hour)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|row| {
            (
                (
                    row.key_kind,
                    row.member_id,
                    row.actor_id,
                    row.key_prefix,
                    row.service,
                    row.route,
                    row.method,
                    row.status,
                    row.cause,
                ),
                row.requests,
            )
        })
        .collect()
}

async fn roll(pool: &PgPool, span: Span, attribution: &Attribution) -> Rewritten {
    let mut tx = pool.begin().await.unwrap();
    let traffic = roll_traffic(&mut tx, span, attribution).await.unwrap();
    let callers = roll_callers(&mut tx, span, attribution).await.unwrap();
    tx.commit().await.unwrap();
    traffic.unwrap().rewritten + callers.unwrap().rewritten
}

async fn fingerprint(pool: &PgPool, span: Span) -> Vec<String> {
    let mut prints = Vec::new();
    for table in ROLLUP_TABLES {
        let print: Option<String> = sqlx::query_scalar(&format!(
            "SELECT md5(string_agg(t::text, '|' ORDER BY t::text)) FROM {name} t
             WHERE {time} >= $1 AND {time} < $2",
            name = table.name,
            time = table.time
        ))
        .bind(span.from)
        .bind(span.to)
        .fetch_one(pool)
        .await
        .unwrap();
        prints.push(print.unwrap());
    }
    prints
}

async fn generate_volume(pool: &PgPool, span: Span, keys: &Keys) {
    let step_ms = (span.to - span.from).num_milliseconds() / i64::from(VOLUME_ROWS);
    sqlx::query(
        "INSERT INTO api_request_log (ts, request_id, method, path, route, status, outcome_status,
           cause, key_kind, member_id, actor_id, key_prefix, user_agent, error, subjects, latency_ms)
         SELECT $1 + make_interval(secs => g * $2 / 1000.0),
                CASE g % 4 WHEN 0 THEN NULL WHEN 1 THEN gen_random_uuid() ELSE uuidv7() END,
                (ARRAY['GET', 'GET', 'POST'])[1 + g % 3],
                (ARRAY['/v3/player/tags', '/v3/resolve/n' || g % 7, '/wp-admin', '/v3/players',
                       '/api/v1/starfish/heartbeat'])[1 + g % 5],
                CASE WHEN g % 4 > 1
                  THEN (ARRAY['/v3/player/tags', '/v3/resolve/{identifier}', NULL, '/v3/players',
                              '/api/v1/starfish/heartbeat'])[1 + g % 5] END,
                (ARRAY[200, 200, 404, 429, 401, 500, 200])[1 + g % 7],
                CASE WHEN g % 4 > 1 AND g % 11 = 0 THEN 429 END,
                CASE WHEN g % 4 > 1 THEN g % 9 END,
                CASE WHEN g % 4 > 1
                  THEN (ARRAY['personal', 'invalid', 'internal', 'session', 'none'])[1 + g % 5]
                  ELSE (ARRAY['key', 'none'])[1 + g % 2] END,
                (ARRAY[$3, NULL, $4])[1 + g % 3],
                CASE WHEN g % 6 = 0 THEN g % 13 END,
                (ARRAY[$5, $6, $7, NULL])[1 + g % 4],
                (ARRAY['coral-bot/1', 'Mozilla/5.0', NULL])[1 + g % 3],
                CASE WHEN g % 3 = 0 THEN $8 END,
                CASE WHEN g % 4 > 1 THEN g % 5 END,
                (g * 37) % 40000
         FROM generate_series(0, $9 - 1) AS g",
    )
    .bind(span.from)
    .bind(step_ms)
    .bind(keys.member)
    .bind(keys.retired_owner)
    .bind(&keys.personal)
    .bind(&keys.unknown)
    .bind(&keys.internal)
    .bind(UPSTREAM_LIMITED)
    .bind(VOLUME_ROWS)
    .execute(pool)
    .await
    .unwrap();
}

async fn hourly_mismatches(pool: &PgPool, span: Span) -> Vec<String> {
    sqlx::query_scalar(
        "WITH raw AS (
           SELECT date_trunc('hour', ts, 'UTC') AS hour, count(*) AS requests,
                  sum(latency_ms) AS latency
           FROM api_request_log WHERE ts >= $1 AND ts < $2 GROUP BY 1),
         minutes AS (
           SELECT date_trunc('hour', minute, 'UTC') AS hour, sum(requests) AS requests,
                  sum(latency_sum_ms) AS latency,
                  sum((SELECT sum(b) FROM unnest(latency_buckets) AS b)) AS bucketed
           FROM api_traffic_minute WHERE minute >= $1 AND minute < $2 GROUP BY 1),
         hours AS (
           SELECT hour, sum(requests) AS requests, sum(latency_sum_ms) AS latency
           FROM api_traffic_hour WHERE hour >= $1 AND hour < $2 GROUP BY 1),
         callers AS (
           SELECT hour, sum(requests) AS requests, sum(latency_sum_ms) AS latency,
                  sum(subjects) AS subjects
           FROM api_caller_hour WHERE hour >= $1 AND hour < $2 GROUP BY 1),
         raw_subjects AS (
           SELECT date_trunc('hour', ts, 'UTC') AS hour, sum(coalesce(subjects, 0)) AS subjects
           FROM api_request_log WHERE ts >= $1 AND ts < $2 GROUP BY 1)
         SELECT raw.hour::text FROM raw
         LEFT JOIN minutes USING (hour) LEFT JOIN hours USING (hour)
         LEFT JOIN callers USING (hour) LEFT JOIN raw_subjects USING (hour)
         WHERE (raw.requests, raw.requests, raw.requests, raw.requests, raw.latency, raw.latency,
                raw.latency, raw_subjects.subjects)
               IS DISTINCT FROM
               (minutes.requests, minutes.bucketed, hours.requests, callers.requests,
                minutes.latency, hours.latency, callers.latency, callers.subjects)",
    )
    .bind(span.from)
    .bind(span.to)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn hours_differing_from_their_minutes(pool: &PgPool, span: Span) -> i64 {
    sqlx::query_scalar(
        "WITH folded AS (
           SELECT date_trunc('hour', minute, 'UTC') AS hour, route, method, status, cause, key_kind,
                  sum(requests)::int AS requests, sum(latency_sum_ms)::bigint AS latency_sum_ms,
                  max(latency_max_ms) AS latency_max_ms
           FROM api_traffic_minute WHERE minute >= $1 AND minute < $2 GROUP BY 1, 2, 3, 4, 5, 6),
         stored AS (
           SELECT hour, route, method, status, cause, key_kind, requests, latency_sum_ms,
                  latency_max_ms
           FROM api_traffic_hour WHERE hour >= $1 AND hour < $2)
         SELECT (SELECT count(*) FROM (SELECT * FROM folded EXCEPT SELECT * FROM stored) AS a)
              + (SELECT count(*) FROM (SELECT * FROM stored EXCEPT SELECT * FROM folded) AS b)",
    )
    .bind(span.from)
    .bind(span.to)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn legacy_paths_map_to_the_same_template_in_sql_and_rust() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let paths = [
        "/",
        "/v3/players",
        "/v3/players/",
        "/v3/resolve/oJugg",
        "/v3/resolve/",
        "/v3/resolve/a/b",
        "/v3/player/sessions/markers/x",
        "/api/v1/starfish/plugins/disabled",
        "/api/v1/starfish/plugins/bedwars",
        "/api/v1/starfish/plugins/bedwars/releases/1.2.0/yank",
        "/openapi.json",
        "/openapiXjson",
        "/wp-login.php",
        "",
    ];
    let mapped: Vec<(String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT c.path, {} FROM unnest($1::text[]) AS c(path)",
        rows::legacy_route("c.path")
    ))
    .bind(&paths[..])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(mapped.len(), paths.len());
    for (path, template) in mapped {
        assert_eq!(template.as_deref(), template_for_path(&path), "{path:?}");
    }
}

#[tokio::test]
async fn status_causes_match_in_sql_and_rust() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let causes: Vec<(i32, Option<String>, i16)> = sqlx::query_as(&format!(
        "SELECT s, r, {} FROM generate_series(100, 599) AS s,
           (VALUES (NULL::text), ('/v3/players')) AS m(r)",
        rows::status_cause("s", "r")
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(causes.len(), 1_000);
    for (status, route, code) in causes {
        assert_eq!(
            Cause::from_code(code),
            Some(Cause::from_status(status as u16, route.is_some())),
            "{status} {route:?}"
        );
    }
}

#[tokio::test]
async fn services_come_from_the_same_agent_product_in_sql_and_rust() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let agents = [
        Some("coral-bot"),
        Some("coral-bot/1.2"),
        Some(" coral-sync /x"),
        Some("\tmc-verify"),
        Some("mc-verify2"),
        Some("Mozilla/5.0"),
        Some(""),
        None,
    ];
    let services: Vec<(Option<String>, i16)> = sqlx::query_as(&format!(
        "SELECT a.user_agent, {} FROM unnest($1::text[]) AS a(user_agent)",
        rows::service_from_agent("a.user_agent")
    ))
    .bind(&agents[..])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(services.len(), agents.len());
    for (agent, code) in services {
        assert_eq!(
            Service::from_code(code),
            Some(Service::from_user_agent(agent.as_deref())),
            "{agent:?}"
        );
    }
}

#[tokio::test]
async fn every_row_expression_attributes_legacy_and_recorded_rows() {
    let Some(pool) = fixture(ROW_DAY).await else {
        return;
    };
    let hour = Span::new(on(ROW_DAY, 10, 0), on(ROW_DAY, 11, 0)).unwrap();
    clear(&pool, hour).await;
    let keys = keys(&pool).await;
    let cases = cases(&keys);
    for (seed, _) in &cases {
        insert(&pool, ROW_DAY, seed).await;
    }

    let attribution = Attribution::with_internal_prefix(keys.internal.clone());
    roll(&pool, hour, &attribution).await;

    let mut wanted: HashMap<CallerKey, i32> = HashMap::new();
    for (seed, expected) in &cases {
        *wanted.entry(caller_key(seed, expected)).or_default() += 1;
    }
    assert_eq!(caller_rows(&pool, hour.from).await, wanted);
    let subjects: i64 =
        sqlx::query_scalar("SELECT sum(subjects)::bigint FROM api_caller_hour WHERE hour = $1")
            .bind(hour.from)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(subjects, 2);
    remove_members(&pool, &keys.members).await;
}

#[tokio::test]
async fn closed_hours_sum_to_the_raw_counts_and_recompute_identically() {
    let Some(pool) = fixture(VOLUME_DAY).await else {
        return;
    };
    let span = Span::new(
        on(VOLUME_DAY, 12, 0),
        on(VOLUME_DAY, 12, 0) + TimeDelta::hours(VOLUME_HOURS),
    )
    .unwrap();
    clear(&pool, span).await;
    let keys = keys(&pool).await;
    generate_volume(&pool, span, &keys).await;
    let attribution = Attribution::with_internal_prefix(keys.internal.clone());

    let first = roll(&pool, span, &attribution).await;
    assert!(first.written > 0);
    assert_eq!(first.removed, 0);
    assert_eq!(hourly_mismatches(&pool, span).await, Vec::<String>::new());
    assert_eq!(hours_differing_from_their_minutes(&pool, span).await, 0);

    let before = fingerprint(&pool, span).await;
    let again = roll(&pool, span, &attribution).await;
    assert_eq!(again, Rewritten::default());
    assert_eq!(fingerprint(&pool, span).await, before);

    let mut chunked = Rewritten::default();
    for chunk in span.chunks(TimeDelta::minutes(37)) {
        chunked = chunked + roll_traffic_only(&pool, chunk, &attribution).await;
    }
    assert_eq!(chunked, Rewritten::default());

    sqlx::query("DELETE FROM api_request_log WHERE ts >= $1 AND ts < $2 AND (latency_ms % 10) = 3")
        .bind(span.from)
        .bind(span.to)
        .execute(&pool)
        .await
        .unwrap();
    let shrunk = roll(&pool, span, &attribution).await;
    assert!(shrunk.written > 0 && shrunk.removed > 0, "{shrunk:?}");
    assert_eq!(hourly_mismatches(&pool, span).await, Vec::<String>::new());
    assert_eq!(hours_differing_from_their_minutes(&pool, span).await, 0);
    remove_members(&pool, &keys.members).await;
}

async fn roll_traffic_only(pool: &PgPool, span: Span, attribution: &Attribution) -> Rewritten {
    let mut tx = pool.begin().await.unwrap();
    let rolled = roll_traffic(&mut tx, span, attribution).await.unwrap();
    tx.commit().await.unwrap();
    rolled.unwrap().rewritten
}

#[tokio::test]
async fn spans_older_than_the_retained_log_keep_their_rollups() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let ancient = Span::new(
        Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2000, 1, 1, 1, 0, 0).unwrap(),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO api_caller_hour (hour, key_kind, service, method, status, cause, requests,
           latency_sum_ms, subjects)
         VALUES ($1, 1, 0, 'GET', 200, 0, 5, 10, 0)",
    )
    .bind(ancient.from)
    .execute(&mut *tx)
    .await
    .unwrap();
    let attribution = Attribution::without_internal_key();
    assert_eq!(
        roll_callers(&mut tx, ancient, &attribution).await.unwrap(),
        None
    );
    assert_eq!(
        roll_traffic(&mut tx, ancient, &attribution).await.unwrap(),
        None
    );
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM api_caller_hour WHERE hour = $1")
        .bind(ancient.from)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(kept, 1);
}

#[tokio::test]
async fn the_network_pass_folds_each_row_once_and_moves_its_watermark() {
    let _state = NETWORK_STATE.lock().await;
    let Some(pool) = fixture(NETWORK_DAY).await else {
        return;
    };
    let span = Span::new(on(NETWORK_DAY, 10, 0), on(NETWORK_DAY, 12, 0)).unwrap();
    clear(&pool, span).await;
    let (member_id, discord_id) = new_member(&pool).await;
    let detailed = Uuid::new_v4();
    let member_row = |minute, key_kind, ip, country, request_id| NetworkRow {
        minute,
        member_id: Some(member_id),
        key_kind,
        actor_id: None,
        ip,
        country,
        user_agent: "ua-a",
        request_id,
    };
    let rows = [
        member_row(5, "key", "198.51.100.7", None, Some(detailed)),
        member_row(40, "personal", "198.51.100.7", Some("NL"), None),
        member_row(20, "personal", "not-an-ip", None, None),
        NetworkRow {
            member_id: None,
            actor_id: Some(discord_id),
            user_agent: "ua-b",
            ..member_row(70, "session", "2001:db8::7", None, None)
        },
        member_row(110, "personal", "198.51.100.7", None, None),
    ];
    for row in rows {
        sqlx::query(
            "INSERT INTO api_request_log (ts, request_id, method, path, status, key_kind, member_id,
               actor_id, ip, country, user_agent, latency_ms)
             VALUES ($1, $2, 'GET', '/v3/player/tags', 200, $3, $4, $5, $6, $7, $8, 1)",
        )
        .bind(span.from + TimeDelta::minutes(row.minute))
        .bind(row.request_id)
        .bind(row.key_kind)
        .bind(row.member_id)
        .bind(row.actor_id)
        .bind(row.ip)
        .bind(row.country)
        .bind(row.user_agent)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO api_request_detail (request_id, ts, headers, body_bytes) VALUES ($1, $2, $3, 0)",
    )
    .bind(detailed)
    .bind(span.from + TimeDelta::minutes(5))
    .bind(json!({ "cf-ipcountry": "DE", "cf-worker": "proxy.example" }))
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    set_watermark(&mut tx, NETWORK_WATERMARK, span.from)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let until = span.from + TimeDelta::minutes(90);
    let mut folded = Vec::new();
    loop {
        let mut tx = pool.begin().await.unwrap();
        let advanced = advance_network(&mut tx, until).await.unwrap();
        tx.commit().await.unwrap();
        match advanced {
            Some(step) => folded.push(step),
            None => break,
        }
    }
    assert_eq!(
        folded,
        vec![
            Span::new(span.from, span.from + HOUR).unwrap(),
            Span::new(span.from + HOUR, until).unwrap(),
        ]
    );

    let ips: Vec<(String, i64, Option<String>, bool)> = sqlx::query_as(
        "SELECT host(ip_address), requests, country, proxied FROM api_key_ips
         WHERE member_id = $1 ORDER BY 1",
    )
    .bind(member_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        ips,
        vec![
            ("198.51.100.7".to_owned(), 2, Some("NL".to_owned()), true),
            ("2001:db8::7".to_owned(), 1, None, false),
        ]
    );
    let agents: Vec<(String, i64)> = sqlx::query_as(
        "SELECT user_agent, requests FROM api_member_agents WHERE member_id = $1 ORDER BY 1",
    )
    .bind(member_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(agents, vec![("ua-a".to_owned(), 3), ("ua-b".to_owned(), 1)]);

    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        watermark(&mut tx, NETWORK_WATERMARK).await.unwrap(),
        Some(until)
    );
    sqlx::query("DELETE FROM api_rollup_state WHERE name = $1")
        .bind(NETWORK_WATERMARK)
        .execute(&mut *tx)
        .await
        .unwrap();
    let floor = log_floor(&mut tx).await.unwrap().unwrap();
    let first = advance_network(&mut tx, floor + TimeDelta::minutes(30))
        .await
        .unwrap();
    assert_eq!(first, Span::new(floor, floor + TimeDelta::minutes(30)));
    tx.rollback().await.unwrap();
    sqlx::query("DELETE FROM api_rollup_state WHERE name = $1")
        .bind(NETWORK_WATERMARK)
        .execute(&pool)
        .await
        .unwrap();
    remove_members(&pool, &[member_id]).await;
}

#[test]
fn dirty_rows_merge_into_touching_whole_hours() {
    let at = |hour, minute| on(ROW_DAY, hour, minute);
    let rows = vec![
        DirtyRow {
            id: 1,
            from_ts: at(10, 5),
            to_ts: at(10, 20),
        },
        DirtyRow {
            id: 2,
            from_ts: at(10, 50),
            to_ts: at(11, 0),
        },
        DirtyRow {
            id: 3,
            from_ts: at(14, 0),
            to_ts: at(14, 0),
        },
    ];
    assert_eq!(
        merge_dirty(rows),
        vec![
            DirtyRange {
                ids: vec![1, 2],
                span: Span::new(at(10, 0), at(12, 0)).unwrap(),
            },
            DirtyRange {
                ids: vec![3],
                span: Span::new(at(14, 0), at(15, 0)).unwrap(),
            },
        ]
    );
}

#[tokio::test]
async fn a_null_key_and_an_empty_key_stay_separate_groups() {
    let Some(pool) = fixture(ROW_DAY).await else {
        return;
    };
    let hour = Span::new(on(ROW_DAY, 15, 0), on(ROW_DAY, 16, 0)).unwrap();
    clear(&pool, hour).await;
    for (minute, route) in [(1, None), (2, Some(""))] {
        sqlx::query(
            "INSERT INTO api_request_log (ts, request_id, method, path, route, status, cause,
               key_kind, latency_ms, subjects)
             VALUES ($1, $2, 'GET', '/x', $3, 404, $4, 'none', 1, 0)",
        )
        .bind(on(ROW_DAY, 15, minute))
        .bind(crate::request_id_for(on(ROW_DAY, 15, minute)))
        .bind(route)
        .bind(Cause::Unmatched.code())
        .execute(&pool)
        .await
        .unwrap();
    }
    let attribution = Attribution::without_internal_key();
    roll(&pool, hour, &attribution).await;
    let routes = |table: &'static str| {
        let pool = pool.clone();
        async move {
            let routes: Vec<Option<String>> = sqlx::query_scalar(&format!(
                "SELECT route FROM {table} WHERE hour = $1 ORDER BY route NULLS FIRST"
            ))
            .bind(hour.from)
            .fetch_all(&pool)
            .await
            .unwrap();
            routes
        }
    };
    let both = vec![None, Some(String::new())];
    assert_eq!(routes("api_caller_hour").await, both);
    assert_eq!(routes("api_traffic_hour").await, both);
    assert_eq!(roll(&pool, hour, &attribution).await, Rewritten::default());

    sqlx::query("DELETE FROM api_request_log WHERE ts >= $1 AND ts < $2 AND route = ''")
        .bind(hour.from)
        .bind(hour.to)
        .execute(&pool)
        .await
        .unwrap();
    let shrunk = roll(&pool, hour, &attribution).await;
    assert_eq!(shrunk.written, 0);
    assert!(shrunk.removed > 0);
    assert_eq!(routes("api_caller_hour").await, vec![None]);
    assert_eq!(routes("api_traffic_hour").await, vec![None]);
}

#[tokio::test]
async fn an_hour_of_thousands_of_groups_in_one_key_kind_rerolls_without_a_quadratic_match() {
    let Some(pool) = fixture(CARDINALITY_DAY).await else {
        return;
    };
    let hour = Span::new(on(CARDINALITY_DAY, 9, 0), on(CARDINALITY_DAY, 10, 0)).unwrap();
    clear(&pool, hour).await;
    sqlx::query(
        "INSERT INTO api_request_log (ts, request_id, method, path, route, status, cause,
           key_kind, latency_ms, subjects)
         SELECT $1 + make_interval(secs => g * 3600.0 / $2), uuidv7(), 'GET', '/scan/' || g,
                '/scan/' || g, 404, $3, 'none', 1, 0
         FROM generate_series(0, $2 - 1) AS g",
    )
    .bind(hour.from)
    .bind(DISTINCT_CALLER_GROUPS)
    .bind(Cause::Unmatched.code())
    .execute(&pool)
    .await
    .unwrap();
    let attribution = Attribution::without_internal_key();
    let first = roll(&pool, hour, &attribution).await;
    let groups_per_table = i64::from(DISTINCT_CALLER_GROUPS);
    assert_eq!(
        first,
        Rewritten {
            written: groups_per_table * ROLLUP_TABLES.len() as i64,
            removed: 0,
        }
    );

    let started = Instant::now();
    let again = roll(&pool, hour, &attribution).await;
    let took = started.elapsed();
    assert_eq!(again, Rewritten::default());
    assert!(took < HIGH_CARDINALITY_BOUND, "re-roll took {took:?}");
    clear(&pool, hour).await;
}

#[tokio::test]
async fn the_internal_prefix_comes_from_the_newest_request_recorded_with_the_internal_key() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let ahead = floor_to(Utc::now() + TimeDelta::days(2), HOUR);
    create_day_partitions(&pool, ahead.date_naive()).await;
    let hour = Span::new(ahead, ahead + HOUR).unwrap();
    clear(&pool, hour).await;
    let (recorded, older, unrecorded) = (random_prefix(), random_prefix(), random_prefix());
    for (minute, request_id, prefix) in [
        (10, Some(crate::request_id_for(ahead)), &recorded),
        (5, Some(crate::request_id_for(ahead)), &older),
        (20, Some(Uuid::new_v4()), &unrecorded),
        (30, None, &unrecorded),
    ] {
        sqlx::query(
            "INSERT INTO api_request_log (ts, request_id, method, path, status, key_kind,
               key_prefix, latency_ms)
             VALUES ($1, $2, 'GET', '/v3/players', 200, 'internal', $3, 1)",
        )
        .bind(ahead + TimeDelta::minutes(minute))
        .bind(request_id)
        .bind(prefix)
        .execute(&pool)
        .await
        .unwrap();
    }

    let mut conn = pool.acquire().await.unwrap();
    let attribution = Attribution::recorded(&mut conn).await.unwrap();
    assert_eq!(attribution.internal_prefix(), Some(recorded.as_str()));
    clear(&pool, hour).await;
}
