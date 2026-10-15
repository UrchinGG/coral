use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderName, HeaderValue, Response};
use axum::middleware::{Next, from_fn};
use axum::response::IntoResponse;
use tower_http::compression::CompressionLayer;
use tower_http::trace::{MakeSpan, TraceLayer};
use tracing::Span;

use crate::error::AdminError;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const SLOW_REQUEST: Duration = Duration::from_millis(500);

const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");
const UNMATCHED_ROUTE: &str = "unmatched";
const MICROS_PER_MILLI: f64 = 1_000.0;

#[derive(Debug, Clone, Copy)]
struct RouteSpan;

pub fn apply<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(from_fn(request_timeout))
        .layer(from_fn(server_timing))
        .layer(CompressionLayer::new().br(true).gzip(true))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(RouteSpan)
                .on_request(())
                .on_response(record_response),
        )
}

impl<B> MakeSpan<B> for RouteSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> Span {
        let route = request
            .extensions()
            .get::<MatchedPath>()
            .map_or(UNMATCHED_ROUTE, MatchedPath::as_str);
        tracing::info_span!(
            "request",
            method = %request.method(),
            route,
            status = tracing::field::Empty,
            latency_ms = tracing::field::Empty,
        )
    }
}

fn record_response<B>(response: &Response<B>, latency: Duration, span: &Span) {
    span.record("status", response.status().as_u16());
    span.record("latency_ms", latency.as_millis() as u64);
    if latency >= SLOW_REQUEST {
        tracing::info!("slow request");
    } else {
        tracing::debug!("request served");
    }
}

async fn request_timeout(request: Request, next: Next) -> axum::response::Response {
    match tokio::time::timeout(REQUEST_TIMEOUT, next.run(request)).await {
        Ok(response) => response,
        Err(_) => AdminError::Timeout(format!(
            "the request did not finish within {} s",
            REQUEST_TIMEOUT.as_secs()
        ))
        .into_response(),
    }
}

async fn server_timing(request: Request, next: Next) -> axum::response::Response {
    let started = Instant::now();
    let mut response = next.run(request).await;
    let millis = started.elapsed().as_micros() as f64 / MICROS_PER_MILLI;
    let timing = HeaderValue::try_from(format!("app;dur={millis:.1}"))
        .expect("a formatted duration is a valid header value");
    response.headers_mut().insert(SERVER_TIMING, timing);
    response
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::body::{Body, to_bytes};
    use axum::http::{StatusCode, header};
    use axum::response::sse::{Event, Sse};
    use axum::routing::get;
    use futures_util::stream;
    use tower::ServiceExt;

    use super::*;

    const LARGE_BODY_BYTES: usize = 4096;
    const ERROR_BODY_LIMIT: usize = 1024;

    fn app() -> Router {
        apply(
            Router::new()
                .route("/large", get(|| async { "x".repeat(LARGE_BODY_BYTES) }))
                .route(
                    "/events",
                    get(|| async {
                        Sse::new(stream::iter([Ok::<_, Infallible>(
                            Event::default().data("x".repeat(LARGE_BODY_BYTES)),
                        )]))
                    }),
                ),
        )
    }

    async fn get_with(path: &str, encoding: &str) -> axum::response::Response {
        app()
            .oneshot(
                axum::http::Request::get(path)
                    .header(header::ACCEPT_ENCODING, encoding)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    fn header_of<'a>(response: &'a axum::response::Response, name: &str) -> Option<&'a str> {
        response
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap())
    }

    #[tokio::test]
    async fn every_response_carries_server_timing() {
        let response = get_with("/large", "identity").await;
        let timing = header_of(&response, "server-timing").unwrap();
        assert!(timing.starts_with("app;dur="), "{timing}");
        let missing = get_with("/nowhere", "identity").await;
        assert!(header_of(&missing, "server-timing").is_some());
    }

    #[tokio::test]
    async fn bodies_are_compressed_with_brotli_or_gzip() {
        let br = get_with("/large", "br").await;
        assert_eq!(header_of(&br, "content-encoding"), Some("br"));
        let gzip = get_with("/large", "gzip").await;
        assert_eq!(header_of(&gzip, "content-encoding"), Some("gzip"));
    }

    #[tokio::test]
    async fn event_streams_are_never_compressed() {
        let events = get_with("/events", "br, gzip").await;
        assert_eq!(header_of(&events, "content-encoding"), None);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_handlers_time_out_with_a_json_504() {
        let app = apply(Router::new().route(
            "/slow",
            get(|| async {
                tokio::time::sleep(REQUEST_TIMEOUT * 2).await;
                "late"
            }),
        ));
        let response = app
            .oneshot(
                axum::http::Request::get("/slow")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        assert!(header_of(&response, "server-timing").is_some());
        let body = to_bytes(response.into_body(), ERROR_BODY_LIMIT)
            .await
            .unwrap();
        let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(error["error"]["code"], "timeout");
        assert_eq!(
            error["error"]["message"],
            "the request did not finish within 15 s"
        );
    }
}
