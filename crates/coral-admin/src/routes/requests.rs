use axum::{Json, Router, extract::*, routing::get};
use chrono::{DateTime, Utc};
use coral_redis::{RateLimiter, SESSION_RATE_LIMIT, SESSION_UUID_BUDGET};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};

use crate::identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/stats", get(stats))
        .route("/series", get(series))
        .route("/hypixel-series", get(hypixel_series))
        .route("/paths", get(paths))
        .route("/ratelimits", get(ratelimits))
        .route("/budgets", get(budgets))
}

#[derive(Deserialize)]
struct ListParams {
    hours: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
    method: Option<String>,
    path: Option<String>,
    path_exact: Option<bool>,
    status: Option<String>,
    key_prefix: Option<String>,
    ip: Option<String>,
    discord_id: Option<i64>,
    caller: Option<String>,
    error_contains: Option<String>,
    errors: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

fn parse_status_filter(input: &str) -> Option<(i16, i16)> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((lo, hi)) = s.split_once('-') {
        let lo: i16 = lo.trim().parse().ok()?;
        let hi: i16 = hi.trim().parse().ok()?;
        return Some((lo.min(hi), lo.max(hi)));
    }
    if s.len() == 3 && s.to_ascii_lowercase().ends_with("xx") {
        let class: i16 = s[..1].parse().ok()?;
        return Some((class * 100, class * 100 + 99));
    }
    let v: i16 = s.parse().ok()?;
    Some((v, v))
}

#[derive(Default)]
struct CallerMatch {
    discord_ids: Vec<i64>,
    uuid: Option<String>,
    ip: Option<String>,
}

impl CallerMatch {
    fn is_empty(&self) -> bool {
        self.discord_ids.is_empty() && self.uuid.is_none() && self.ip.is_none()
    }
}

async fn resolve_caller(state: &AppState, query: &str) -> CallerMatch {
    let query = query.trim();
    if query.is_empty() {
        return CallerMatch::default();
    }
    if let Ok(id) = query.parse::<i64>() {
        return CallerMatch {
            discord_ids: vec![id],
            uuid: None,
            ip: None,
        };
    }
    if clients::is_uuid(query) {
        return CallerMatch {
            discord_ids: vec![],
            uuid: Some(clients::normalize_uuid(query)),
            ip: None,
        };
    }
    if query.parse::<std::net::IpAddr>().is_ok() {
        return CallerMatch {
            discord_ids: vec![],
            uuid: None,
            ip: Some(query.to_string()),
        };
    }

    let discord_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT discord_id FROM discord_username_cache WHERE username ILIKE $1 LIMIT 25",
    )
    .bind(format!("%{query}%"))
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let uuid = if identity::looks_like_ign(query) {
        identity::resolve_minecraft_uuid(state, query).await
    } else {
        None
    };

    CallerMatch {
        discord_ids,
        uuid,
        ip: None,
    }
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
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    discord_id: Option<i64>,
    uuid: Option<String>,
    #[sqlx(default)]
    discord_username: Option<String>,
    #[sqlx(default)]
    minecraft_username: Option<String>,
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    requests: Vec<RequestRow>,
}

fn filters(qb: &mut QueryBuilder<'_, Postgres>, p: &ListParams, caller: &CallerMatch) {
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
        if p.path_exact.unwrap_or(false) {
            qb.push(" AND l.path = ").push_bind(path.to_string());
        } else {
            qb.push(" AND l.path LIKE ").push_bind(format!("%{path}%"));
        }
    }
    if let Some(status_input) = p.status.as_deref().filter(|s| !s.is_empty()) {
        match parse_status_filter(status_input) {
            Some((lo, hi)) if lo == hi => {
                qb.push(" AND l.status = ").push_bind(lo);
            }
            Some((lo, hi)) => {
                qb.push(" AND l.status BETWEEN ")
                    .push_bind(lo)
                    .push(" AND ")
                    .push_bind(hi);
            }
            None => {
                qb.push(" AND false");
            }
        }
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
    if let Some(discord_id) = p.discord_id {
        qb.push(" AND m.discord_id = ").push_bind(discord_id);
    }
    if let Some(q) = p.error_contains.as_deref().filter(|s| !s.trim().is_empty()) {
        qb.push(" AND l.error ILIKE ").push_bind(format!("%{q}%"));
    }
    if p.caller.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        if caller.is_empty() {
            qb.push(" AND false");
        } else {
            let mut sep = " AND (";
            if !caller.discord_ids.is_empty() {
                qb.push(sep)
                    .push("m.discord_id = ANY(")
                    .push_bind(caller.discord_ids.clone())
                    .push(")");
                sep = " OR ";
            }
            if let Some(uuid) = &caller.uuid {
                qb.push(sep).push("m.uuid = ").push_bind(uuid.clone());
                sep = " OR ";
            }
            if let Some(ip) = &caller.ip {
                qb.push(sep).push("l.ip = ").push_bind(ip.clone());
            }
            qb.push(")");
        }
    }
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let pool = state.db.pool();
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    let offset = params.offset.unwrap_or(0).max(0);

    let caller = match params.caller.as_deref().map(str::trim) {
        Some(q) if !q.is_empty() => resolve_caller(&state, q).await,
        _ => CallerMatch::default(),
    };

    let mut count = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*) FROM api_request_log l LEFT JOIN members m ON m.id = l.member_id",
    );
    filters(&mut count, &params, &caller);
    let total = count
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let mut q = QueryBuilder::<Postgres>::new(
        "SELECT l.ts, l.method, l.path, l.query, l.status, l.latency_ms, l.key_prefix, l.ip,
                l.user_agent, l.error, m.discord_id, m.uuid
         FROM api_request_log l
         LEFT JOIN members m ON m.id = l.member_id",
    );
    filters(&mut q, &params, &caller);
    q.push(" ORDER BY l.ts DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let mut requests = q
        .build_query_as::<RequestRow>()
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let discord_ids: Vec<i64> = requests.iter().filter_map(|r| r.discord_id).collect();
    let uuids: Vec<String> = requests.iter().filter_map(|r| r.uuid.clone()).collect();
    let names = identity::resolve(&state, &discord_ids, &uuids).await;
    for r in &mut requests {
        r.discord_username = r.discord_id.and_then(|id| names.discord.get(&id).cloned());
        r.minecraft_username = r
            .uuid
            .as_ref()
            .and_then(|u| names.minecraft.get(u).cloned());
    }

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
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    discord_id: Option<i64>,
    uuid: Option<String>,
    count: i64,
    errors: i64,
    rate_limited: i64,
    forbidden: i64,
    #[sqlx(default)]
    discord_username: Option<String>,
    #[sqlx(default)]
    minecraft_username: Option<String>,
}

#[derive(Serialize, FromRow)]
struct TopPath {
    path: Option<String>,
    count: i64,
    errors: i64,
    avg_ms: Option<f64>,
    p50_ms: Option<f64>,
    p95_ms: Option<f64>,
    p99_ms: Option<f64>,
    status_2xx: i64,
    status_3xx: i64,
    status_4xx: i64,
    status_5xx: i64,
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

    let mut top_keys = sqlx::query_as::<_, TopKey>(
        "SELECT l.key_prefix, m.discord_id, m.uuid, count(*) AS count,
                count(*) FILTER (WHERE l.status >= 400) AS errors,
                count(*) FILTER (WHERE l.status = 429) AS rate_limited,
                count(*) FILTER (WHERE l.status = 403) AS forbidden
         FROM api_request_log l
         LEFT JOIN members m ON m.id = l.member_id
         WHERE l.ts > now() - make_interval(hours => $1)
         GROUP BY l.key_prefix, m.discord_id, m.uuid ORDER BY count DESC LIMIT 15",
    )
    .bind(h)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let discord_ids: Vec<i64> = top_keys.iter().filter_map(|k| k.discord_id).collect();
    let uuids: Vec<String> = top_keys.iter().filter_map(|k| k.uuid.clone()).collect();
    let names = identity::resolve(&state, &discord_ids, &uuids).await;
    for k in &mut top_keys {
        k.discord_username = k.discord_id.and_then(|id| names.discord.get(&id).cloned());
        k.minecraft_username = k
            .uuid
            .as_ref()
            .and_then(|u| names.minecraft.get(u).cloned());
    }

    let top_paths = sqlx::query_as::<_, TopPath>(&format!(
        "SELECT {NORM_PATH} AS path, count(*) AS count,
                count(*) FILTER (WHERE status >= 400) AS errors,
                avg(latency_ms)::float8 AS avg_ms,
                percentile_cont(0.5) WITHIN GROUP (ORDER BY latency_ms)::float8 AS p50_ms,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms)::float8 AS p95_ms,
                percentile_cont(0.99) WITHIN GROUP (ORDER BY latency_ms)::float8 AS p99_ms,
                count(*) FILTER (WHERE status BETWEEN 200 AND 299) AS status_2xx,
                count(*) FILTER (WHERE status BETWEEN 300 AND 399) AS status_3xx,
                count(*) FILTER (WHERE status BETWEEN 400 AND 499) AS status_4xx,
                count(*) FILTER (WHERE status BETWEEN 500 AND 599) AS status_5xx
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

#[derive(Serialize, Default, Clone, Copy)]
pub(crate) struct RateLimits {
    pub(crate) available: bool,
    pub(crate) capacity: i64,
    pub(crate) used: i64,
    pub(crate) headroom: i64,
}

impl RateLimits {
    pub(crate) fn headroom_ratio(&self) -> Option<f64> {
        (self.available && self.capacity > 0).then(|| self.headroom as f64 / self.capacity as f64)
    }
}

async fn ratelimits(State(state): State<AppState>) -> Json<RateLimits> {
    Json(hypixel_ratelimits(&state).await)
}

pub(crate) async fn hypixel_ratelimits(state: &AppState) -> RateLimits {
    let Some(redis) = state.redis.clone() else {
        return RateLimits::default();
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
    view
}

#[derive(Serialize)]
struct BudgetRow {
    #[serde(serialize_with = "crate::serde_id::discord_id")]
    discord_id: i64,
    discord_username: Option<String>,
    kind: &'static str,
    used: i64,
    limit: i64,
    utilization: f64,
}

async fn budgets(State(state): State<AppState>) -> Json<Vec<BudgetRow>> {
    let Some(redis) = state.redis.clone() else {
        return Json(vec![]);
    };
    let limiter = RateLimiter::new(redis);

    let (session_usage, uuid_usage) = tokio::join!(
        limiter.scan_budgets("sf:", SESSION_RATE_LIMIT),
        limiter.scan_budgets("sfuuids:", SESSION_UUID_BUDGET),
    );

    let mut rows: Vec<BudgetRow> = session_usage
        .unwrap_or_default()
        .into_iter()
        .filter_map(|b| budget_row(b, "sf:", "session"))
        .chain(
            uuid_usage
                .unwrap_or_default()
                .into_iter()
                .filter_map(|b| budget_row(b, "sfuuids:", "uuid_batch")),
        )
        .collect();

    let discord_ids: Vec<i64> = rows.iter().map(|r| r.discord_id).collect();
    let names = identity::resolve_discord_usernames(&state, &discord_ids).await;
    for row in &mut rows {
        row.discord_username = names.get(&row.discord_id).cloned();
    }

    rows.sort_by(|a, b| b.utilization.partial_cmp(&a.utilization).unwrap());
    Json(rows)
}

fn budget_row(
    usage: coral_redis::BudgetUsage,
    prefix: &str,
    kind: &'static str,
) -> Option<BudgetRow> {
    let discord_id: i64 = usage.name.strip_prefix(prefix)?.parse().ok()?;
    Some(BudgetRow {
        discord_id,
        discord_username: None,
        kind,
        used: usage.used,
        limit: usage.limit,
        utilization: usage.utilization(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_status_code() {
        assert_eq!(parse_status_filter("429"), Some((429, 429)));
        assert_eq!(parse_status_filter("200"), Some((200, 200)));
    }

    #[test]
    fn parses_explicit_range() {
        assert_eq!(parse_status_filter("400-499"), Some((400, 499)));
        assert_eq!(parse_status_filter("500-599"), Some((500, 599)));
    }

    #[test]
    fn normalizes_reversed_range() {
        assert_eq!(parse_status_filter("499-400"), Some((400, 499)));
    }

    #[test]
    fn parses_status_class_shorthand() {
        assert_eq!(parse_status_filter("4xx"), Some((400, 499)));
        assert_eq!(parse_status_filter("5xx"), Some((500, 599)));
        assert_eq!(parse_status_filter("2XX"), Some((200, 299)));
    }

    #[test]
    fn rejects_invalid_status_input() {
        assert_eq!(parse_status_filter(""), None);
        assert_eq!(parse_status_filter("  "), None);
        assert_eq!(parse_status_filter("abc"), None);
        assert_eq!(parse_status_filter("xx4"), None);
    }

    #[test]
    fn caller_match_empty_detection() {
        assert!(CallerMatch::default().is_empty());
        assert!(
            !CallerMatch {
                discord_ids: vec![1],
                uuid: None,
                ip: None,
            }
            .is_empty()
        );
        assert!(
            !CallerMatch {
                discord_ids: vec![],
                uuid: Some("abc".into()),
                ip: None,
            }
            .is_empty()
        );
        assert!(
            !CallerMatch {
                discord_ids: vec![],
                uuid: None,
                ip: Some("1.2.3.4".into()),
            }
            .is_empty()
        );
    }
}
