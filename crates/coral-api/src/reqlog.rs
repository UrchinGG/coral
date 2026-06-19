use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use chrono::{DateTime, Utc};
use database::Database;
use sqlx::{PgPool, Postgres, QueryBuilder};
use tokio::sync::mpsc;

pub struct LogEntry {
    ts: DateTime<Utc>,
    ip: Option<String>,
    key_prefix: Option<String>,
    key_kind: &'static str,
    method: String,
    path: String,
    status: i16,
    latency_ms: i32,
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
    let ip = client_ip(&req);
    let key = extract_key(&req);
    let key_kind = if key.is_some() { "key" } else { "none" };
    let key_prefix = key.map(|k| k.chars().take(8).collect());
    let start = Instant::now();

    let res = next.run(req).await;

    let _ = tx.try_send(LogEntry {
        ts: Utc::now(),
        ip,
        key_prefix,
        key_kind,
        method,
        path,
        status: res.status().as_u16() as i16,
        latency_ms: start.elapsed().as_millis().min(i32::MAX as u128) as i32,
    });
    res
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
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO api_request_log (ts, ip, key_prefix, key_kind, method, path, status, latency_ms) ",
    );
    qb.push_values(entries, |mut b, e| {
        b.push_bind(e.ts)
            .push_bind(e.ip.clone())
            .push_bind(e.key_prefix.clone())
            .push_bind(e.key_kind)
            .push_bind(e.method.clone())
            .push_bind(e.path.clone())
            .push_bind(e.status)
            .push_bind(e.latency_ms);
    });
    qb.build().execute(pool).await?;
    Ok(())
}

pub fn spawn_partition_maintenance(db: Arc<Database>) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = maintain_partitions(db.pool()).await {
                tracing::warn!("api request-log partition maintenance failed: {e}");
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

async fn maintain_partitions(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"DO $$
        DECLARE d date := current_date; r record;
        BEGIN
          FOR i IN 0..2 LOOP
            EXECUTE format(
              'CREATE TABLE IF NOT EXISTS api_request_log_%s PARTITION OF api_request_log FOR VALUES FROM (%L) TO (%L)',
              to_char(d + i, 'YYYYMMDD'), (d + i)::text, (d + i + 1)::text);
          END LOOP;
          FOR r IN
            SELECT c.relname AS name FROM pg_inherits inh
            JOIN pg_class c ON c.oid = inh.inhrelid
            WHERE inh.inhparent = 'api_request_log'::regclass
              AND c.relname ~ '^api_request_log_[0-9]{8}$'
              AND to_date(right(c.relname, 8), 'YYYYMMDD') < current_date - 14
          LOOP
            EXECUTE format('DROP TABLE IF EXISTS %I', r.name);
          END LOOP;
        END $$;"#,
    )
    .execute(pool)
    .await?;
    Ok(())
}
