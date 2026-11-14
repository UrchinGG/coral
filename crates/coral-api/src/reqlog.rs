use std::env;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use axum::body::{Body, Bytes, HttpBody};
use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde_json::{Map, Value};
use tokio::task::JoinHandle;

use reqlog::{
    Caller, Cause, Client, Detail, KEY_PREFIX_CHARS, KeyKind, LogEntry, MAX_REQUEST_BODY, Recorded,
    RequestContext, SHUTDOWN_DRAIN, STORED_BODY_BYTES, SUBJECT_SOURCES_ENV, StoredText,
    SubjectSources, WriterConfig,
};

pub use reqlog::RequestLog;

use crate::state::AppState;

const HEALTH_PATH: &str = "/health";
const REQUEST_ID_HEADER: &str = "x-request-id";
const API_KEY_HEADER: &str = "x-api-key";
const SESSION_HEADER: &str = "x-starfish-session";
const AUTHORIZATION_HEADER: &str = "authorization";
const CLIENT_IP_HEADER: &str = "cf-connecting-ip";
const FORWARDED_FOR_HEADER: &str = "x-forwarded-for";
const COUNTRY_HEADER: &str = "cf-ipcountry";
const USER_AGENT_HEADER: &str = "user-agent";
const REDACTED: &str = "[redacted]";
const KEY_QUERY_PREFIX: &str = "key=";
const FIRST_ERROR_STATUS: u16 = StatusCode::BAD_REQUEST.as_u16();

struct RequestFacts {
    method: String,
    path: String,
    route: Option<String>,
    query: Option<String>,
    client: Client,
    credential_presented: bool,
    headers: Value,
}

enum ReceivedBody {
    Complete(Bytes),
    TooLarge { prefix: Bytes, size: usize },
    Interrupted { prefix: Bytes, size: usize },
}

struct Exchange {
    facts: RequestFacts,
    body: ReceivedBody,
    status: u16,
    latency_ms: i32,
    error_body: Option<String>,
    recorded: Recorded,
}

pub fn start(state: &AppState) -> Result<(RequestLog, JoinHandle<()>)> {
    let sources = subject_sources()?;
    let pool = state.db.pool().clone();
    let partitions_ready = reqlog::partitions::spawn_maintenance(pool.clone());
    let (log, receiver) = reqlog::channel();
    let writer = reqlog::spawn_writer(
        WriterConfig {
            pool,
            redis: state.redis.connection(),
            sources,
            partitions_ready,
        },
        receiver,
    );
    Ok((log, writer))
}

pub async fn drain(writer: JoinHandle<()>) {
    match tokio::time::timeout(SHUTDOWN_DRAIN, writer).await {
        Ok(Ok(())) => tracing::info!("request log drained"),
        Ok(Err(e)) => tracing::error!("request log writer failed: {e}"),
        Err(_) => tracing::warn!("request log drain timed out; unwritten entries were lost"),
    }
}

pub async fn log_requests(State(log): State<RequestLog>, request: Request, next: Next) -> Response {
    if request.uri().path() == HEALTH_PATH {
        return next.run(request).await;
    }
    let started = Instant::now();
    let facts = RequestFacts::capture(&request);
    let (parts, body) = request.into_parts();
    let body = ReceivedBody::read(body).await;
    let context = Arc::new(RequestContext::default());
    let response = match &body {
        ReceivedBody::Complete(bytes) => {
            let request = Request::from_parts(parts, Body::from(bytes.clone()));
            reqlog::scope(context.clone(), next.run(request)).await
        }
        ReceivedBody::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        ReceivedBody::Interrupted { .. } => StatusCode::BAD_REQUEST.into_response(),
    };
    let latency_ms = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
    let status = response.status().as_u16();
    let (mut response, error_body) = capture_error_body(response).await;
    let exchange = Exchange {
        facts,
        body,
        status,
        latency_ms,
        error_body,
        recorded: context.take(),
    };
    let entry = exchange.into_entry();
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&entry.request_id.to_string())
            .expect("a uuid is always a valid header value"),
    );
    log.submit(entry);
    response
}

impl RequestFacts {
    fn capture(request: &Request) -> Self {
        let headers = request.headers();
        let key = api_key(request);
        Self {
            method: request.method().as_str().to_owned(),
            path: request.uri().path().to_owned(),
            route: request
                .extensions()
                .get::<MatchedPath>()
                .map(|matched| matched.as_str().to_owned()),
            query: request.uri().query().map(str::to_owned),
            client: Client {
                ip: client_ip(headers),
                country: header_text(headers, COUNTRY_HEADER),
                user_agent: header_text(headers, USER_AGENT_HEADER),
                key_prefix: key
                    .as_deref()
                    .map(|key| key.chars().take(KEY_PREFIX_CHARS).collect()),
            },
            credential_presented: key.is_some()
                || headers.contains_key(SESSION_HEADER)
                || headers.contains_key(AUTHORIZATION_HEADER),
            headers: header_json(headers),
        }
    }
}

impl ReceivedBody {
    async fn read(body: Body) -> Self {
        let declared = body.size_hint().exact();
        let mut stream = body.into_data_stream();
        let mut buffer = Vec::new();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else {
                let size = buffer.len();
                return Self::Interrupted {
                    prefix: Bytes::from(buffer),
                    size,
                };
            };
            if buffer.len() + chunk.len() > MAX_REQUEST_BODY {
                return Self::too_large(buffer, &chunk, declared);
            }
            buffer.extend_from_slice(&chunk);
        }
        Self::Complete(Bytes::from(buffer))
    }

    fn too_large(mut prefix: Vec<u8>, overflow: &[u8], declared: Option<u64>) -> Self {
        let read = prefix.len() + overflow.len();
        let size = declared.map_or(read, |declared| (declared as usize).max(read));
        let room = STORED_BODY_BYTES.saturating_sub(prefix.len());
        prefix.extend_from_slice(&overflow[..room.min(overflow.len())]);
        Self::TooLarge {
            prefix: Bytes::from(prefix),
            size,
        }
    }

    fn bytes(&self) -> &Bytes {
        match self {
            Self::Complete(bytes) => bytes,
            Self::TooLarge { prefix, .. } | Self::Interrupted { prefix, .. } => prefix,
        }
    }

    fn size(&self) -> usize {
        match self {
            Self::Complete(bytes) => bytes.len(),
            Self::TooLarge { size, .. } | Self::Interrupted { size, .. } => *size,
        }
    }
}

impl Exchange {
    fn into_entry(self) -> LogEntry {
        let ts = reqlog::now();
        let route_matched = self.facts.route.is_some();
        let outcome = self.recorded.outcome;
        let semantic_status = outcome.map_or(self.status, |outcome| outcome.status);
        let cause = outcome
            .map(|outcome| outcome.cause)
            .or(self.recorded.cause)
            .unwrap_or_else(|| Cause::from_status(semantic_status, route_matched));
        let caller = self.recorded.caller.unwrap_or_else(|| {
            Caller::anonymous(match self.facts.credential_presented {
                true => KeyKind::Unverified,
                false => KeyKind::None,
            })
        });
        let error = match self.status {
            FIRST_ERROR_STATUS.. => self.error_body,
            _ => outcome.map(|outcome| outcome.reason.to_owned()),
        };
        let accepted = route_matched && semantic_status < FIRST_ERROR_STATUS;
        LogEntry {
            request_id: reqlog::request_id_for(ts),
            ts,
            method: self.facts.method,
            path: self.facts.path,
            route: self.facts.route,
            query: self.facts.query,
            client: self.facts.client,
            caller,
            status: self.status,
            outcome_status: outcome
                .map(|outcome| outcome.status)
                .filter(|status| *status != self.status),
            cause,
            latency_ms: self.latency_ms,
            error,
            detail: Detail::capture(
                self.facts.headers,
                self.body.bytes(),
                self.body.size(),
                accepted,
                self.recorded.server_error,
            ),
        }
    }
}

async fn capture_error_body(response: Response) -> (Response, Option<String>) {
    if response.status().as_u16() < FIRST_ERROR_STATUS {
        return (response, None);
    }
    let (parts, body) = response.into_parts();
    match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => {
            let error = (!bytes.is_empty()).then(|| StoredText::decode(&bytes).text);
            (Response::from_parts(parts, Body::from(bytes)), error)
        }
        Err(e) => {
            tracing::warn!("error response body could not be read for the request log: {e}");
            (Response::from_parts(parts, Body::empty()), None)
        }
    }
}

fn header_json(headers: &HeaderMap) -> Value {
    let mut out = Map::new();
    for (name, value) in headers {
        let value = match name.as_str() {
            AUTHORIZATION_HEADER => REDACTED.into(),
            _ => String::from_utf8_lossy(value.as_bytes()),
        };
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

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
}

pub(crate) fn client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(CLIENT_IP_HEADER)
        .or_else(|| headers.get(FORWARDED_FOR_HEADER))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse().ok())
}

fn api_key(request: &Request) -> Option<String> {
    if let Some(header) = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
    {
        return Some(header.to_owned());
    }
    request
        .uri()
        .query()?
        .split('&')
        .find_map(|pair| pair.strip_prefix(KEY_QUERY_PREFIX).map(str::to_owned))
}

fn subject_sources() -> Result<SubjectSources> {
    let Ok(raw) = env::var(SUBJECT_SOURCES_ENV) else {
        return Ok(SubjectSources::All);
    };
    raw.parse()
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid {SUBJECT_SOURCES_ENV}"))
}

#[cfg(test)]
mod tests;
