use std::cell::Cell;

use axum::extract::Request;
use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderMap, HeaderName};
use axum::middleware::Next;
use axum::response::Response;
use coral_redis::RateLimitResult;

tokio::task_local! {
    static OBSERVED: Cell<Option<RateLimitResult>>;
}

const LIMIT: HeaderName = HeaderName::from_static("x-ratelimit-limit");
const REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");
const RESET: HeaderName = HeaderName::from_static("x-ratelimit-reset");

pub fn admit(result: RateLimitResult) -> bool {
    observe(result);
    matches!(result, RateLimitResult::Allowed { .. })
}

pub fn observe(result: RateLimitResult) {
    if let RateLimitResult::Allowed { limit, .. } = result
        && limit <= 0
    {
        return;
    }
    let _ = OBSERVED.try_with(|observed| observed.set(Some(tighter(observed.get(), result))));
}

pub async fn add_headers(request: Request, next: Next) -> Response {
    OBSERVED
        .scope(Cell::new(None), async move {
            let mut response = next.run(request).await;
            if let Some(result) = OBSERVED.with(Cell::get) {
                write_headers(response.headers_mut(), result);
            }
            response
        })
        .await
}

fn tighter(current: Option<RateLimitResult>, next: RateLimitResult) -> RateLimitResult {
    use RateLimitResult::{Allowed, Exceeded};
    let Some(current) = current else {
        return next;
    };
    match (current, next) {
        (Exceeded { .. }, Allowed { .. }) => current,
        (Allowed { .. }, Exceeded { .. }) => next,
        (
            Exceeded {
                retry_after: held, ..
            },
            Exceeded {
                retry_after: new, ..
            },
        ) => {
            if new > held {
                next
            } else {
                current
            }
        }
        (
            Allowed {
                remaining: held, ..
            },
            Allowed { remaining: new, .. },
        ) => {
            if new < held {
                next
            } else {
                current
            }
        }
    }
}

fn write_headers(headers: &mut HeaderMap, result: RateLimitResult) {
    let (limit, remaining, reset) = match result {
        RateLimitResult::Allowed {
            limit,
            remaining,
            reset_after,
        } => (limit, remaining, reset_after),
        RateLimitResult::Exceeded { limit, retry_after } => {
            headers.insert(RETRY_AFTER, retry_after.into());
            (limit, 0, retry_after)
        }
    };
    headers.insert(LIMIT, limit.into());
    headers.insert(REMAINING, remaining.max(0).into());
    headers.insert(RESET, reset.into());
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    fn allowed(limit: i64, remaining: i64, reset_after: i64) -> RateLimitResult {
        RateLimitResult::Allowed {
            limit,
            remaining,
            reset_after,
        }
    }

    fn exceeded(limit: i64, retry_after: i64) -> RateLimitResult {
        RateLimitResult::Exceeded { limit, retry_after }
    }

    async fn headers_for(results: Vec<RateLimitResult>) -> (StatusCode, HeaderMap) {
        let app = Router::new()
            .route(
                "/",
                get(move || async move {
                    let mut status = StatusCode::OK;
                    for result in results {
                        if !admit(result) {
                            status = StatusCode::TOO_MANY_REQUESTS;
                        }
                    }
                    status.into_response()
                }),
            )
            .layer(axum::middleware::from_fn(add_headers));
        let response = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        (response.status(), response.headers().clone())
    }

    fn header(headers: &HeaderMap, name: &str) -> Option<String> {
        headers
            .get(name)
            .map(|value| value.to_str().unwrap().to_owned())
    }

    #[tokio::test]
    async fn allowed_requests_report_the_remaining_quota() {
        let (status, headers) = headers_for(vec![allowed(600, 412, 87)]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            header(&headers, "x-ratelimit-limit").as_deref(),
            Some("600")
        );
        assert_eq!(
            header(&headers, "x-ratelimit-remaining").as_deref(),
            Some("412")
        );
        assert_eq!(header(&headers, "x-ratelimit-reset").as_deref(), Some("87"));
        assert_eq!(header(&headers, "retry-after"), None);
    }

    #[tokio::test]
    async fn exceeded_requests_carry_retry_after() {
        let (status, headers) = headers_for(vec![allowed(600, 500, 40), exceeded(30, 132)]).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(header(&headers, "retry-after").as_deref(), Some("132"));
        assert_eq!(header(&headers, "x-ratelimit-limit").as_deref(), Some("30"));
        assert_eq!(
            header(&headers, "x-ratelimit-remaining").as_deref(),
            Some("0")
        );
        assert_eq!(
            header(&headers, "x-ratelimit-reset").as_deref(),
            Some("132")
        );
    }

    #[tokio::test]
    async fn the_tighter_of_two_limits_wins() {
        let (_, headers) = headers_for(vec![allowed(600, 590, 10), allowed(30, 4, 200)]).await;
        assert_eq!(header(&headers, "x-ratelimit-limit").as_deref(), Some("30"));
        assert_eq!(
            header(&headers, "x-ratelimit-remaining").as_deref(),
            Some("4")
        );
    }

    #[tokio::test]
    async fn unlimited_results_add_no_headers() {
        let (_, headers) = headers_for(vec![allowed(0, i64::MAX, 0)]).await;
        assert_eq!(header(&headers, "x-ratelimit-limit"), None);
    }

    #[tokio::test]
    async fn requests_without_a_limit_add_no_headers() {
        let (_, headers) = headers_for(vec![]).await;
        assert_eq!(header(&headers, "x-ratelimit-limit"), None);
    }

    #[test]
    fn observing_outside_a_request_is_ignored() {
        observe(exceeded(30, 10));
    }
}
