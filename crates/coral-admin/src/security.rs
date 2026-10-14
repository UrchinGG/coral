use std::sync::Arc;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use url::Url;

use crate::auth::AuthConfig;

pub const ROOT_PATH: &str = "/";

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";
const REFERRER_POLICY: &str = "no-referrer";
const NO_SNIFF: &str = "nosniff";
const NO_STORE: &str = "no-store";
const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const SAME_ORIGIN_SITE: &str = "same-origin";
const SCHEME_RELATIVE_PREFIX: &str = "//";

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

pub fn safe_next(raw: &str, base: &Url) -> String {
    same_origin_target(raw, base).unwrap_or_else(|| ROOT_PATH.to_string())
}

pub fn json_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
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

fn same_origin_target(raw: &str, base: &Url) -> Option<String> {
    if raw.chars().any(|c| c.is_control() || c == '\\') {
        return None;
    }
    let target = base.join(raw).ok()?;
    let leaves_origin = target.scheme() != base.scheme() || target.origin() != base.origin();
    if leaves_origin || target.path().starts_with(SCHEME_RELATIVE_PREFIX) {
        return None;
    }
    Some(match target.query() {
        Some(query) => format!("{}?{query}", target.path()),
        None => target.path().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://admin.example";

    fn next(raw: &str) -> String {
        safe_next(raw, &Url::parse(BASE).unwrap())
    }

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
    fn rejects_values_that_leave_the_origin() {
        for raw in [
            "/\t/evil.example",
            "//evil",
            "/\\evil",
            "https://evil",
            "///evil",
            "javascript:alert(1)",
            "http://admin.example/members",
            "https://admin.example:8443/members",
            "/.//evil",
            "/a/..//evil",
            "/\r\n/evil",
            "/\u{7f}evil",
            "blob:https://admin.example//evil.example/",
            "blob:https://admin.example/members?q=1",
        ] {
            assert_eq!(next(raw), "/", "{raw:?}");
        }
    }

    #[test]
    fn percent_encoded_tricks_stay_on_the_admin_origin() {
        assert_eq!(next("/%09/evil.example"), "/%09/evil.example");
        assert_eq!(next("/%2F%2Fevil"), "/%2F%2Fevil");
    }

    #[test]
    fn keeps_same_origin_path_and_query_without_fragment() {
        assert_eq!(next("/members?q=a&b=1#top"), "/members?q=a&b=1");
        assert_eq!(next("https://admin.example/players/x"), "/players/x");
        assert_eq!(next("/a/../members"), "/members");
    }

    #[test]
    fn empty_next_resolves_to_root() {
        assert_eq!(next(""), "/");
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
