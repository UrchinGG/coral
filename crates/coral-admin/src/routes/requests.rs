use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/stats", get(stats))
        .route("/series", get(series))
        .route("/hypixel-series", get(hypixel_series))
        .route("/paths", get(paths))
        .route("/ratelimits", get(ratelimits))
}

#[derive(Deserialize)]
struct ListParams {
    hours: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
    method: Option<String>,
    path: Option<String>,
    status: Option<i16>,
    key_prefix: Option<String>,
    ip: Option<String>,
    errors: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize, FromRow)]
struct RequestRow {
    ts: DateTime<Utc>,
    method: Option<String>,
    path: Option<String>,
    query: Option<String>,
    status: Option<i16>,
    latency_ms: Option<i32>,
    key_prefix: Option<String>,
    ip: Option<String>,
    user_agent: Option<String>,
    error: Option<String>,
    discord_id: Option<i64>,
    uuid: Option<String>,
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    requests: Vec<RequestRow>,
}

fn filters(qb: &mut QueryBuilder<'_, Postgres>, p: &ListParams) {
    match (p.from, p.to) {
        (Some(from), Some(to)) => {
            qb.push(" WHERE l.ts >= to_timestamp(")
                .push_bind(from)
                .push(") AND l.ts < to_timestamp(")
                .push_bind(to)
                .push(")");
        }
        _ => {
            let hours = p.hours.unwrap_or(24).clamp(1, 336) as i32;
            qb.push(" WHERE l.ts > now() - make_interval(hours => ")
                .push_bind(hours)
                .push(")");
        }
    }
    if let Some(m) = p.method.as_deref().filter(|s| !s.is_empty()) {
        qb.push(" AND l.method = ").push_bind(m.to_string());
    }
    if let Some(path) = p.path.as_deref().filter(|s| !s.is_empty()) {
        qb.push(" AND l.path LIKE ").push_bind(format!("%{path}%"));
    }
    if let Some(s) = p.status {
        qb.push(" AND l.status = ").push_bind(s);
    }
    if p.errors.unwrap_or(false) {
        qb.push(" AND l.status >= 400");
    }
    if let Some(k) = p.key_prefix.as_deref().filter(|s| !s.is_empty()) {
        qb.push(" AND l.key_prefix = ").push_bind(k.to_string());
    }
    if let Some(ip) = p.ip.as_deref().filter(|s| !s.is_empty()) {
        qb.push(" AND l.ip = ").push_bind(ip.to_string());
    }
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let pool = state.db.pool();
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    let offset = params.offset.unwrap_or(0).max(0);

    let mut count = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM api_request_log l");
    filters(&mut count, &params);
    let total = count
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let mut q = QueryBuilder::<Postgres>::new(
        "SELECT l.ts, l.method, l.path, l.query, l.status, l.latency_ms, l.key_prefix, l.ip,
                l.user_agent, l.error, m.discord_id, m.uuid
         FROM api_request_log l
         LEFT JOIN members m ON left(m.api_key, 8) = l.key_prefix",
    );
    filters(&mut q, &params);
    q.push(" ORDER BY l.ts DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let requests = q
        .build_query_as::<RequestRow>()
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    Json(ListResponse { total, requests })
}

fn bucket_interval(hours: i64) -> &'static str {
    match hours {
        ..=1 => "1 minute",
        2..=3 => "2 minutes",
        4..=6 => "5 minutes",
        7..=12 => "10 minutes",
        13..=24 => "20 minutes",
        25..=48 => "30 minutes",
        49..=96 => "1 hour",
        97..=168 => "3 hours",
        _ => "6 hours",
    }
}

fn bucket_seconds(hours: i64) -> i64 {
    match hours {
        ..=1 => 60,
        2..=3 => 120,
        4..=6 => 300,
        7..=12 => 600,
        13..=24 => 1200,
        25..=48 => 1800,
        49..=96 => 3600,
        97..=168 => 10800,
        _ => 21600,
    }
}

const NORM_PATH: &str = "regexp_replace(\
    regexp_replace(\
        regexp_replace(\
            regexp_replace(path, '^/v3/resolve/.+$', '/v3/resolve/{player}'), \
            '/[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}', '/{uuid}', 'g'), \
        '/[0-9a-f]{32}', '/{uuid}', 'g'), \
    '/[0-9]+($|/)', '/{id}\\1', 'g')";

async fn hypixel_series(
    State(state): State<AppState>,
    Query(p): Query<HoursParam>,
) -> Json<Vec<Bucket>> {
    let hours = p.hours.unwrap_or(24).clamp(1, 336);
    let Some(redis) = state.redis.clone() else {
        return Json(vec![]);
    };
    let mut conn = redis.connection();

    let now = Utc::now().timestamp();
    let start = now - hours * 3600;
    let width = bucket_seconds(hours);
    let mut buckets: std::collections::BTreeMap<i64, (i64, i64)> = Default::default();

    for day in (start / 86_400)..=(now / 86_400) {
        let fields: std::collections::HashMap<String, i64> = redis::cmd("HGETALL")
            .arg(format!("hp:hist:{day}"))
            .query_async(&mut conn)
            .await
            .unwrap_or_default();
        for (field, count) in fields {
            let (kind, rest) = field.split_at(1);
            let Ok(minute) = rest.parse::<i64>() else {
                continue;
            };
            let ts = day * 86_400 + minute * 60;
            if ts < start || ts > now {
                continue;
            }
            let entry = buckets.entry((ts / width) * width).or_default();
            match kind {
                "t" => entry.0 += count,
                "e" => entry.1 += count,
                _ => {}
            }
        }
    }

    let out = buckets
        .into_iter()
        .filter_map(|(b, (total, errors))| {
            DateTime::from_timestamp(b, 0).map(|t| Bucket { t, total, errors })
        })
        .collect();
    Json(out)
}

#[derive(Deserialize)]
struct SeriesParams {
    hours: Option<i64>,
    path: Option<String>,
}

#[derive(Serialize, FromRow)]
struct Bucket {
    t: DateTime<Utc>,
    total: i64,
    errors: i64,
}

async fn series(State(state): State<AppState>, Query(p): Query<SeriesParams>) -> Json<Vec<Bucket>> {
    let hours = p.hours.unwrap_or(24).clamp(1, 336);
    let mut q = QueryBuilder::<Postgres>::new("SELECT date_bin(");
    q.push_bind(bucket_interval(hours))
        .push(
            "::interval, ts, timestamptz '2000-01-01') AS t, count(*) AS total, \
               count(*) FILTER (WHERE status >= 400) AS errors \
               FROM api_request_log WHERE ts > now() - make_interval(hours => ",
        )
        .push_bind(hours as i32)
        .push(")");
    if let Some(path) = p.path.as_deref().filter(|s| !s.is_empty()) {
        q.push(format!(" AND {NORM_PATH} = "))
            .push_bind(path.to_string());
    }
    q.push(" GROUP BY t ORDER BY t");

    let buckets = q
        .build_query_as::<Bucket>()
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default();
    Json(buckets)
}

#[derive(Deserialize)]
struct HoursParam {
    hours: Option<i64>,
}

#[derive(Serialize, FromRow)]
struct PathCount {
    path: Option<String>,
    count: i64,
}

async fn paths(State(state): State<AppState>, Query(p): Query<HoursParam>) -> Json<Vec<PathCount>> {
    let hours = p.hours.unwrap_or(24).clamp(1, 336) as i32;
    let rows = sqlx::query_as::<_, PathCount>(&format!(
        "SELECT {NORM_PATH} AS path, count(*) AS count FROM api_request_log
         WHERE ts > now() - make_interval(hours => $1)
         GROUP BY 1 ORDER BY count DESC LIMIT 100"
    ))
    .bind(hours)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();
    Json(rows)
}

#[derive(Serialize, FromRow)]
struct TopKey {
    key_prefix: Option<String>,
    discord_id: Option<i64>,
    uuid: Option<String>,
    count: i64,
    errors: i64,
}

#[derive(Serialize, FromRow)]
struct TopPath {
    path: Option<String>,
    count: i64,
    errors: i64,
    avg_ms: Option<f64>,
}

#[derive(Serialize, FromRow)]
struct StatusClass {
    class: i32,
    count: i64,
}

#[derive(Serialize)]
struct Stats {
    hours: i64,
    total: i64,
    errors: i64,
    avg_ms: Option<f64>,
    status_classes: Vec<StatusClass>,
    top_keys: Vec<TopKey>,
    top_paths: Vec<TopPath>,
}

async fn stats(State(state): State<AppState>, Query(p): Query<HoursParam>) -> Json<Stats> {
    let pool = state.db.pool();
    let hours = p.hours.unwrap_or(24).clamp(1, 336);
    let h = hours as i32;

    let (total, errors, avg_ms) = sqlx::query_as::<_, (i64, i64, Option<f64>)>(
        "SELECT count(*), count(*) FILTER (WHERE status >= 400), avg(latency_ms)::float8
         FROM api_request_log WHERE ts > now() - make_interval(hours => $1)",
    )
    .bind(h)
    .fetch_one(pool)
    .await
    .unwrap_or((0, 0, None));

    let status_classes = sqlx::query_as::<_, StatusClass>(
        "SELECT (status / 100)::int AS class, count(*) AS count
         FROM api_request_log WHERE ts > now() - make_interval(hours => $1)
         GROUP BY class ORDER BY class",
    )
    .bind(h)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let top_keys = sqlx::query_as::<_, TopKey>(
        "SELECT l.key_prefix, m.discord_id, m.uuid, count(*) AS count,
                count(*) FILTER (WHERE l.status >= 400) AS errors
         FROM api_request_log l
         LEFT JOIN members m ON left(m.api_key, 8) = l.key_prefix
         WHERE l.ts > now() - make_interval(hours => $1)
         GROUP BY l.key_prefix, m.discord_id, m.uuid ORDER BY count DESC LIMIT 15",
    )
    .bind(h)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let top_paths = sqlx::query_as::<_, TopPath>(&format!(
        "SELECT {NORM_PATH} AS path, count(*) AS count,
                count(*) FILTER (WHERE status >= 400) AS errors,
                avg(latency_ms)::float8 AS avg_ms
         FROM api_request_log
         WHERE ts > now() - make_interval(hours => $1)
         GROUP BY 1 ORDER BY count DESC LIMIT 15"
    ))
    .bind(h)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Json(Stats {
        hours,
        total,
        errors,
        avg_ms,
        status_classes,
        top_keys,
        top_paths,
    })
}

#[derive(Serialize, Default)]
struct RateLimits {
    available: bool,
    capacity: i64,
    used: i64,
    headroom: i64,
}

async fn ratelimits(State(state): State<AppState>) -> Json<RateLimits> {
    let Some(redis) = state.redis.clone() else {
        return Json(RateLimits::default());
    };
    let mut conn = redis.connection();

    let mut cursor: u64 = 0;
    let mut lim_keys: Vec<String> = Vec::new();
    loop {
        let res: Result<(u64, Vec<String>), _> = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg("hp:rl:*:lim")
            .arg("COUNT")
            .arg(200)
            .query_async(&mut conn)
            .await;
        let Ok((next, batch)) = res else { break };
        lim_keys.extend(batch);
        cursor = next;
        if cursor == 0 {
            break;
        }
    }

    let mut view = RateLimits {
        available: true,
        ..Default::default()
    };
    for lim_key in lim_keys {
        let limit: i64 = conn.get(&lim_key).await.unwrap_or(0);
        let raw: i64 = conn.get(lim_key.replace(":lim", ":n")).await.unwrap_or(0);
        view.capacity += limit;
        view.used += raw.clamp(0, limit.max(0));
    }
    view.headroom = view.capacity - view.used;
    Json(view)
}
