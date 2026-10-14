use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use database::Database;
use serde_json::{Map, Value};
use sqlx::{PgPool, Postgres, QueryBuilder};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::RequestIdentity;

const MAX_REQUEST_BODY: usize = 2 * 1024 * 1024;
const STORED_BODY_BYTES: usize = 64 * 1024;
const KEY_PREFIX_CHARS: usize = 8;
const LOG_TABLE: &str = "api_request_log";
const LOG_RETENTION_DAYS: i32 = 14;
const DETAIL_TABLE: &str = "api_request_detail";
const DETAIL_RETENTION_DAYS: i32 = 14;

pub struct LogEntry {
    request_id: Uuid,
    ts: DateTime<Utc>,
    ip: Option<String>,
    member_id: Option<i64>,
    key_prefix: Option<String>,
    key_kind: &'static str,
    method: String,
    path: String,
    query: Option<String>,
    user_agent: Option<String>,
    status: i16,
    latency_ms: i32,
    error: Option<String>,
    detail: RequestDetail,
}

struct RequestDetail {
    headers: Value,
    body: Option<String>,
    body_bytes: i32,
}

pub type LogSender = mpsc::Sender<LogEntry>;

pub fn channel() -> (LogSender, mpsc::Receiver<LogEntry>) {
    mpsc::channel(10_000)
}

pub async fn log_requests(State(tx): State<LogSender>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    if path == "/health" {
        return next.run(req).await;
    }
    let method = req.method().as_str().to_owned();
    let query = req.uri().query().map(str::to_owned);
    let ip = client_ip(&req);
    let user_agent = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(256).collect());
    let key = extract_key(&req);
    let key_kind = if key.is_some() { "key" } else { "none" };
    let key_prefix = key.map(|k| k.chars().take(KEY_PREFIX_CHARS).collect());
    let headers = header_json(req.headers());
    let start = Instant::now();

    let (parts, body) = req.into_parts();
    let (res, body) = match axum::body::to_bytes(body, MAX_REQUEST_BODY).await {
        Ok(bytes) => {
            let req = Request::from_parts(parts, Body::from(bytes.clone()));
            (next.run(req).await, bytes)
        }
        Err(_) => (StatusCode::PAYLOAD_TOO_LARGE.into_response(), Bytes::new()),
    };
    let member_id = res
        .extensions()
        .get::<RequestIdentity>()
        .map(|i| i.member_id);
    let status = res.status().as_u16() as i16;
    let latency_ms = start.elapsed().as_millis().min(i32::MAX as u128) as i32;

    let (res, error) = capture_error(res, status).await;

    let detail = RequestDetail {
        headers,
        body: stored_body(&body),
        body_bytes: body.len().min(i32::MAX as usize) as i32,
    };
    let _ = tx.try_send(LogEntry {
        request_id: Uuid::new_v4(),
        ts: Utc::now(),
        ip,
        member_id,
        key_prefix,
        key_kind,
        method,
        path,
        query,
        user_agent,
        status,
        latency_ms,
        error,
        detail,
    });
    res
}

fn header_json(headers: &HeaderMap) -> Value {
    let mut out = Map::new();
    for (name, value) in headers {
        let value = String::from_utf8_lossy(value.as_bytes());
        match out.get_mut(name.as_str()) {
            Some(Value::String(existing)) => {
                existing.push_str(", ");
                existing.push_str(&value);
            }
            _ => {
                out.insert(name.as_str().to_owned(), Value::String(value.into_owned()));
            }
        }
    }
    Value::Object(out)
}

fn stored_body(body: &Bytes) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let end = body.len().min(STORED_BODY_BYTES);
    Some(String::from_utf8_lossy(&body[..end]).into_owned())
}

async fn capture_error(res: Response, status: i16) -> (Response, Option<String>) {
    if status < 400 {
        return (res, None);
    }
    let (parts, body) = res.into_parts();
    match axum::body::to_bytes(body, 1_048_576).await {
        Ok(bytes) => {
            let error = (!bytes.is_empty())
                .then(|| String::from_utf8_lossy(&bytes).chars().take(2000).collect());
            (
                Response::from_parts(parts, axum::body::Body::from(bytes)),
                error,
            )
        }
        Err(_) => (Response::from_parts(parts, axum::body::Body::empty()), None),
    }
}

fn client_ip(req: &Request) -> Option<String> {
    let headers = req.headers();
    headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or(v).trim().to_owned())
}

fn extract_key(req: &Request) -> Option<String> {
    if let Some(header) = req.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
        return Some(header.to_owned());
    }
    req.uri()
        .query()?
        .split('&')
        .find_map(|pair| pair.strip_prefix("key=").map(str::to_owned))
}

pub fn spawn_writer(db: Arc<Database>, mut rx: mpsc::Receiver<LogEntry>) {
    tokio::spawn(async move {
        let mut buf = Vec::with_capacity(500);
        while rx.recv_many(&mut buf, 500).await > 0 {
            if let Err(e) = insert_batch(db.pool(), &buf).await {
                tracing::warn!("api request-log insert failed: {e}");
            }
            buf.clear();
        }
    });
}

async fn insert_batch(pool: &PgPool, entries: &[LogEntry]) -> Result<(), sqlx::Error> {
    insert_log(pool, entries).await?;
    insert_detail(pool, entries).await
}

async fn insert_log(pool: &PgPool, entries: &[LogEntry]) -> Result<(), sqlx::Error> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO api_request_log (request_id, ts, ip, member_id, key_prefix, key_kind, method, path, query, user_agent, status, latency_ms, error) ",
    );
    qb.push_values(entries, |mut b, e| {
        b.push_bind(e.request_id)
            .push_bind(e.ts)
            .push_bind(e.ip.clone())
            .push_bind(e.member_id)
            .push_bind(e.key_prefix.clone())
            .push_bind(e.key_kind)
            .push_bind(e.method.clone())
            .push_bind(e.path.clone())
            .push_bind(e.query.clone())
            .push_bind(e.user_agent.clone())
            .push_bind(e.status)
            .push_bind(e.latency_ms)
            .push_bind(e.error.clone());
    });
    qb.build().execute(pool).await?;
    Ok(())
}

async fn insert_detail(pool: &PgPool, entries: &[LogEntry]) -> Result<(), sqlx::Error> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO api_request_detail (request_id, ts, member_id, headers, body, body_bytes) ",
    );
    qb.push_values(entries, |mut b, e| {
        b.push_bind(e.request_id)
            .push_bind(e.ts)
            .push_bind(e.member_id)
            .push_bind(&e.detail.headers)
            .push_bind(e.detail.body.clone())
            .push_bind(e.detail.body_bytes);
    });
    qb.build().execute(pool).await?;
    Ok(())
}

pub fn spawn_partition_maintenance(db: Arc<Database>) {
    tokio::spawn(async move {
        loop {
            for (table, keep_days) in [
                (LOG_TABLE, LOG_RETENTION_DAYS),
                (DETAIL_TABLE, DETAIL_RETENTION_DAYS),
            ] {
                if let Err(e) = maintain_partitions(db.pool(), table, keep_days).await {
                    tracing::warn!("{table} partition maintenance failed: {e}");
                }
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

async fn maintain_partitions(
    pool: &PgPool,
    table: &str,
    keep_days: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        r#"DO $$
        DECLARE d date := current_date; r record;
        BEGIN
          FOR i IN 0..2 LOOP
            EXECUTE format(
              'CREATE TABLE IF NOT EXISTS {table}_%s PARTITION OF {table} FOR VALUES FROM (%L) TO (%L)',
              to_char(d + i, 'YYYYMMDD'), (d + i)::text, (d + i + 1)::text);
          END LOOP;
          FOR r IN
            SELECT c.relname AS name FROM pg_inherits inh
            JOIN pg_class c ON c.oid = inh.inhrelid
            WHERE inh.inhparent = '{table}'::regclass
              AND c.relname ~ '^{table}_[0-9]{{8}}$'
              AND to_date(right(c.relname, 8), 'YYYYMMDD') < current_date - {keep_days}
          LOOP
            EXECUTE format('DROP TABLE IF EXISTS %I', r.name);
          END LOOP;
        END $$;"#
    ))
    .execute(pool)
    .await?;
    Ok(())
}
