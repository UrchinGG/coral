use std::sync::Arc;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use url::Url;

use crate::auth::AuthConfig;

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https://cdn.discordapp.com https://mc-heads.net; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";
const REFERRER_POLICY: &str = "no-referrer";
const NO_SNIFF: &str = "nosniff";
const NO_STORE: &str = "no-store";
const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const SAME_ORIGIN_SITE: &str = "same-origin";

pub async fn headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static(REFERRER_POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static(NO_SNIFF),
    );
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static(NO_STORE));
    response
}

pub async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE));
    response
}

pub async fn same_origin_writes(
    State(auth): State<Arc<AuthConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if request.method().is_safe() || is_same_origin(request.headers(), &auth.base_url) {
        return next.run(request).await;
    }
    json_error(
        StatusCode::FORBIDDEN,
        "cross_origin",
        "Writes must come from the admin origin",
    )
}

pub fn json_error(status: StatusCode, code: &'static str, message: &str) -> Response {
    let body = json!({ "error": { "code": code, "message": message } });
    (status, Json(body)).into_response()
}

fn is_same_origin(headers: &HeaderMap, base: &Url) -> bool {
    match headers.get(SEC_FETCH_SITE) {
        Some(site) => site == SAME_ORIGIN_SITE,
        None => headers
            .get(header::ORIGIN)
            .is_some_and(|origin| origin == base.origin().ascii_serialization().as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://admin.example";

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    HeaderName::from_static(name),
                    HeaderValue::from_static(value),
                )
            })
            .collect()
    }

    fn same_origin(pairs: &[(&'static str, &'static str)]) -> bool {
        is_same_origin(&headers(pairs), &Url::parse(BASE).unwrap())
    }

    #[test]
    fn same_origin_fetch_metadata_is_accepted() {
        assert!(same_origin(&[("sec-fetch-site", "same-origin")]));
    }

    #[test]
    fn other_fetch_metadata_is_rejected_even_with_a_matching_origin() {
        for site in ["same-site", "cross-site", "none"] {
            let pairs = [("sec-fetch-site", site), ("origin", BASE)];
            assert!(!same_origin(&pairs), "{site}");
        }
    }

    #[test]
    fn origin_is_checked_when_fetch_metadata_is_absent() {
        assert!(same_origin(&[("origin", BASE)]));
        assert!(!same_origin(&[("origin", "https://evil.example")]));
        assert!(!same_origin(&[("origin", "null")]));
        assert!(!same_origin(&[]));
    }
}
