use std::sync::Arc;

use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};

use crate::auth::{self, AuthConfig};
use crate::boot::BootSource;
use crate::{layers, routes, security};

pub fn router<S>(auth_config: Arc<AuthConfig>, boot: BootSource, api: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let api = api
        .fallback(routes::api_not_found)
        .layer(from_fn_with_state(
            auth_config.clone(),
            security::same_origin_writes,
        ))
        .layer(from_fn_with_state(auth_config.clone(), auth::require_owner))
        .layer(from_fn(security::no_store));
    let ui = routes::ui_router(boot).layer(from_fn_with_state(
        auth_config.clone(),
        auth::require_owner_page,
    ));
    let app = Router::new()
        .merge(auth::router(auth_config))
        .nest("/api", api)
        .merge(ui)
        .layer(from_fn(security::headers));
    layers::apply(app)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::convert::Infallible;

    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderMap, HeaderName, Request, Response, StatusCode, header, request};
    use axum::response::sse::{Event, Sse};
    use axum::routing::get;
    use futures_util::stream;
    use tower::ServiceExt;
    use url::Url;

    use super::*;
    use crate::coverage::{Coverage, CoverageCache};
    use crate::session::SESSION_COOKIE;
    use crate::session::tests::MemorySessions;

    const OWNER: i64 = 111;
    const STRANGER: i64 = 222;
    const BASE: &str = "https://admin.example";
    const BODY_LIMIT: usize = 1 << 20;
    const LOGIN_PAGE_LIMIT: usize = 2048;
    const BUILD: &str = "test-build";
    const CLEARED_SESSION: &str = "__Secure-urchin_session=; Domain=urchin.gg; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0";
    const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

    struct Harness {
        auth: Arc<AuthConfig>,
        sessions: Arc<MemorySessions>,
    }

    impl Harness {
        fn new() -> Self {
            let sessions = Arc::new(MemorySessions::default());
            let auth = AuthConfig {
                owner_ids: HashSet::from([OWNER]),
                base_url: Url::parse(BASE).unwrap(),
                sessions: sessions.clone(),
            };
            Self {
                auth: Arc::new(auth),
                sessions,
            }
        }

        fn session_cookie(&self, discord_id: i64) -> String {
            format!("{SESSION_COOKIE}={}", self.sessions.sign_in(discord_id))
        }

        fn owner_cookie(&self) -> String {
            self.session_cookie(OWNER)
        }

        async fn send(&self, request: request::Builder) -> Response<Body> {
            let boot = BootSource::new(
                Some(BUILD.into()),
                CoverageCache::fixed(Coverage::default()),
                Vec::new(),
            );
            router(self.auth.clone(), boot, stub_api())
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap()
        }

        async fn send_as_owner(&self, request: request::Builder) -> Response<Body> {
            self.send(request.header(header::COOKIE, self.owner_cookie()))
                .await
        }
    }

    fn stub_api() -> Router {
        let members = Router::new().route("/{id}", get(|| async { "member" }));
        Router::new()
            .route(
                "/ping",
                get(|| async { "pong" }).post(|| async { "posted" }),
            )
            .route(
                "/events",
                get(|| async { Sse::new(stream::empty::<Result<Event, Infallible>>()) }),
            )
            .nest("/members", members)
    }

    fn header_value(response: &Response<Body>, name: header::HeaderName) -> &str {
        response
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap())
            .unwrap_or("")
    }

    fn location(response: &Response<Body>) -> &str {
        header_value(response, header::LOCATION)
    }

    fn set_cookies(response: &Response<Body>) -> Vec<String> {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap().to_string())
            .collect()
    }

    async fn body_text(response: Response<Body>) -> String {
        let bytes = to_bytes(response.into_body(), BODY_LIMIT).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn error_code(response: Response<Body>) -> String {
        let body: serde_json::Value = serde_json::from_str(&body_text(response).await).unwrap();
        body["error"]["code"].as_str().unwrap().to_string()
    }

    fn query_param(url: &str, name: &str) -> String {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .unwrap()
    }

    async fn first_asset_path(harness: &Harness) -> String {
        let index = body_text(harness.send_as_owner(Request::get("/")).await).await;
        let start = index.find("/assets/").unwrap();
        let end = start + index[start..].find('"').unwrap();
        index[start..end].to_string()
    }

    #[tokio::test]
    async fn unauthenticated_page_redirects_to_login_with_next() {
        let harness = Harness::new();
        let response = harness.send(Request::get("/members?q=1")).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&response), "/login?next=%2Fmembers%3Fq%3D1");

        let root = harness.send(Request::get("/")).await;
        assert_eq!(root.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&root), "/login?next=%2F");
    }

    #[tokio::test]
    async fn unauthenticated_assets_get_an_empty_401() {
        let harness = Harness::new();
        for path in ["/assets/index.js", "/assets/nope.css"] {
            let response = harness.send(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(body_text(response).await, "", "{path}");
        }
    }

    #[tokio::test]
    async fn unauthenticated_non_navigation_page_request_gets_401() {
        let harness = Harness::new();
        let response = harness.send(Request::post("/members")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unauthenticated_api_gets_json_401_for_every_path() {
        let harness = Harness::new();
        for path in ["/api/ping", "/api/members/5", "/api/doesnotexist", "/api"] {
            let response = harness.send(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
            assert_eq!(error_code(response).await, "unauthorized", "{path}");
        }
    }

    #[tokio::test]
    async fn unauthenticated_api_write_gets_401_before_the_origin_check() {
        let harness = Harness::new();
        let response = harness.send(Request::post("/api/ping")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn owner_reaches_api_handlers() {
        let harness = Harness::new();
        let ping = harness.send_as_owner(Request::get("/api/ping")).await;
        assert_eq!(ping.status(), StatusCode::OK);
        assert_eq!(header_value(&ping, header::CACHE_CONTROL), "no-store");
        assert_eq!(body_text(ping).await, "pong");

        let member = harness.send_as_owner(Request::get("/api/members/5")).await;
        assert_eq!(body_text(member).await, "member");
    }

    #[tokio::test]
    async fn owner_event_streams_are_not_stored_either() {
        let harness = Harness::new();
        let events = harness.send_as_owner(Request::get("/api/events")).await;
        assert_eq!(events.status(), StatusCode::OK);
        assert_eq!(
            header_value(&events, header::CONTENT_TYPE),
            "text/event-stream"
        );
        assert_eq!(header_value(&events, header::CACHE_CONTROL), "no-store");
    }

    #[tokio::test]
    async fn owner_gets_json_404_for_unknown_api_paths() {
        let harness = Harness::new();
        for path in ["/api/doesnotexist", "/api/members", "/api"] {
            let response = harness.send_as_owner(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
            assert_eq!(error_code(response).await, "not_found", "{path}");
        }
    }

    #[tokio::test]
    async fn owner_writes_need_same_origin_fetch_metadata_or_origin() {
        let harness = Harness::new();
        let cases = [
            (Some("same-origin"), None, StatusCode::OK),
            (Some("same-site"), Some(BASE), StatusCode::FORBIDDEN),
            (Some("cross-site"), None, StatusCode::FORBIDDEN),
            (None, Some(BASE), StatusCode::OK),
            (None, Some("https://evil.example"), StatusCode::FORBIDDEN),
            (None, None, StatusCode::FORBIDDEN),
        ];
        for (site, origin, expected) in cases {
            let mut request = Request::post("/api/ping");
            if let Some(site) = site {
                request = request.header("sec-fetch-site", site);
            }
            if let Some(origin) = origin {
                request = request.header(header::ORIGIN, origin);
            }
            let response = harness.send_as_owner(request).await;
            assert_eq!(response.status(), expected, "{site:?} {origin:?}");
        }
    }

    #[tokio::test]
    async fn rejected_write_reports_a_json_error() {
        let harness = Harness::new();
        let request = Request::post("/api/ping").header("sec-fetch-site", "cross-site");
        let response = harness.send_as_owner(request).await;
        assert_eq!(error_code(response).await, "cross_origin");
    }

    #[tokio::test]
    async fn owner_gets_index_html_without_caching_for_spa_paths() {
        let harness = Harness::new();
        for path in ["/", "/members/12", "/starfish/plugins/x?tab=y"] {
            let response = harness.send_as_owner(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert!(header_value(&response, header::CONTENT_TYPE).starts_with("text/html"));
            assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
            assert!(body_text(response).await.contains("<div id=\"root\">"));
        }
    }

    #[tokio::test]
    async fn owner_index_carries_the_boot_payload_for_the_signed_in_owner() {
        let harness = Harness::new();
        let index = body_text(harness.send_as_owner(Request::get("/players/x")).await).await;
        let open = "<script id=\"boot\" type=\"application/json\">";
        let start = index.find(open).unwrap() + open.len();
        let end = start + index[start..].find("</script>").unwrap();
        let boot: serde_json::Value = serde_json::from_str(&index[start..end]).unwrap();
        assert_eq!(boot["discord_id"], OWNER.to_string());
        assert_eq!(boot["build"], BUILD);
        assert!(boot["coverage"].is_object());
    }

    #[tokio::test]
    async fn every_response_reports_server_timing() {
        let harness = Harness::new();
        for response in [
            harness.send(Request::get("/api/ping")).await,
            harness.send_as_owner(Request::get("/api/ping")).await,
            harness.send(Request::get("/")).await,
        ] {
            assert!(
                header_value(&response, HeaderName::from_static("server-timing"))
                    .starts_with("app;dur=")
            );
        }
    }

    #[tokio::test]
    async fn owner_gets_embedded_assets_with_private_immutable_caching() {
        let harness = Harness::new();
        let index = body_text(harness.send_as_owner(Request::get("/")).await).await;
        let start = index.find("/assets/").unwrap();
        let end = start + index[start..].find('"').unwrap();
        let response = harness
            .send_as_owner(Request::get(&index[start..end]))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            header_value(&response, header::CACHE_CONTROL),
            "private, max-age=31536000, immutable"
        );
    }

    #[tokio::test]
    async fn owner_gets_404_for_missing_assets() {
        let harness = Harness::new();
        let response = harness.send_as_owner(Request::get("/assets/nope.js")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_text(response).await, "");
    }

    #[tokio::test]
    async fn brotli_clients_get_the_asset_precompressed_at_build_time() {
        let harness = Harness::new();
        let path = first_asset_path(&harness).await;
        let plain = harness.send_as_owner(Request::get(&path)).await;
        let brotli = harness
            .send_as_owner(Request::get(&path).header(header::ACCEPT_ENCODING, "gzip, br"))
            .await;
        assert_eq!(header_value(&brotli, header::CONTENT_ENCODING), "br");
        assert!(plain.headers().get(header::CONTENT_ENCODING).is_none());
        for response in [&plain, &brotli] {
            assert_eq!(header_value(response, header::VARY), "accept-encoding");
        }
        assert_eq!(
            header_value(&brotli, header::CONTENT_TYPE),
            header_value(&plain, header::CONTENT_TYPE)
        );
        let plain_len = to_bytes(plain.into_body(), BODY_LIMIT).await.unwrap().len();
        let brotli_len = to_bytes(brotli.into_body(), BODY_LIMIT)
            .await
            .unwrap()
            .len();
        assert!(brotli_len < plain_len);
    }

    #[tokio::test]
    async fn non_owner_session_is_treated_as_unauthenticated() {
        let harness = Harness::new();
        let cookie = harness.session_cookie(STRANGER);
        let api = harness
            .send(Request::get("/api/ping").header(header::COOKIE, &cookie))
            .await;
        assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
        let page = harness
            .send(Request::get("/").header(header::COOKIE, &cookie))
            .await;
        assert_eq!(page.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn unknown_empty_or_misnamed_session_cookies_are_rejected() {
        let harness = Harness::new();
        let token = harness.sessions.sign_in(OWNER);
        for cookie in [
            format!("{SESSION_COOKIE}=forged"),
            format!("{SESSION_COOKIE}="),
            format!("urchin_session={token}"),
        ] {
            let request = Request::get("/api/ping").header(header::COOKIE, cookie);
            assert_eq!(
                harness.send(request).await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }

    #[tokio::test]
    async fn logout_ends_the_shared_session_and_clears_its_cookie() {
        let harness = Harness::new();
        let token = harness.sessions.sign_in(OWNER);
        let other_browser = harness.sessions.sign_in(OWNER);
        let logout = Request::post("/auth/logout")
            .header(header::COOKIE, format!("{SESSION_COOKIE}={token}"))
            .header("sec-fetch-site", "same-origin");
        let response = harness.send(logout).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&response), "/login");
        assert_eq!(set_cookies(&response), [CLEARED_SESSION]);
        assert!(!harness.sessions.is_live(&token));
        assert!(harness.sessions.is_live(&other_browser));
    }

    #[tokio::test]
    async fn logout_without_a_session_still_clears_the_cookie() {
        let harness = Harness::new();
        for cookie in [None, Some(format!("{SESSION_COOKIE}=forged"))] {
            let mut request = Request::post("/auth/logout").header("sec-fetch-site", "same-origin");
            if let Some(cookie) = &cookie {
                request = request.header(header::COOKIE, cookie);
            }
            let response = harness.send(request).await;
            assert_eq!(location(&response), "/login", "{cookie:?}");
            assert_eq!(set_cookies(&response), [CLEARED_SESSION]);
        }
    }

    #[tokio::test]
    async fn logout_requires_same_origin_and_a_post() {
        let harness = Harness::new();
        let token = harness.sessions.sign_in(OWNER);
        let cookie = format!("{SESSION_COOKIE}={token}");
        let cross_site = harness
            .send(
                Request::post("/auth/logout")
                    .header(header::COOKIE, &cookie)
                    .header("sec-fetch-site", "same-site"),
            )
            .await;
        assert_eq!(cross_site.status(), StatusCode::FORBIDDEN);
        let by_link = harness
            .send(Request::get("/auth/logout").header(header::COOKIE, &cookie))
            .await;
        assert_eq!(by_link.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(harness.sessions.is_live(&token));
    }

    #[tokio::test]
    async fn login_page_is_small_script_free_html() {
        let harness = Harness::new();
        let response = harness.send(Request::get("/login")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(header_value(&response, header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
        let page = body_text(response).await;
        assert!(page.len() <= LOGIN_PAGE_LIMIT, "{} bytes", page.len());
        assert!(!page.contains("<script"));
        assert!(!page.contains("class=\"notice\""));
    }

    #[tokio::test]
    async fn login_page_signs_in_on_the_site_and_returns_to_a_validated_next() {
        let harness = Harness::new();
        let cases = [
            ("/login", "https://admin.example/"),
            (
                "/login?next=%2Fmembers%3Fq%3D1",
                "https://admin.example/members?q=1",
            ),
            ("/login?next=//evil", "https://admin.example/"),
            ("/login?next=/%09/evil.example", "https://admin.example/"),
            ("/login?next=https://evil", "https://admin.example/"),
            (
                "/login?next=/members&next=/players",
                "https://admin.example/",
            ),
        ];
        for (uri, return_to) in cases {
            let page = body_text(harness.send(Request::get(uri)).await).await;
            let start = page.find("href=\"").unwrap() + "href=\"".len();
            let href = &page[start..start + page[start..].find('"').unwrap()];
            assert!(
                href.starts_with("https://api.urchin.gg/auth/discord?"),
                "{uri}: {href}"
            );
            assert_eq!(query_param(href, "return"), return_to, "{uri}");
        }
    }

    #[tokio::test]
    async fn signed_in_stranger_is_told_and_offered_a_sign_out() {
        let harness = Harness::new();
        let cookie = harness.session_cookie(STRANGER);
        let response = harness
            .send(Request::get("/login?next=/members").header(header::COOKIE, cookie))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let page = body_text(response).await;
        assert!(page.contains("That Discord account is not an owner."));
        assert!(page.contains("<form method=\"post\" action=\"/auth/logout\">"));
        assert!(!page.contains("api.urchin.gg"));
    }

    #[tokio::test]
    async fn signed_in_owner_on_login_page_is_sent_to_a_safe_next() {
        let harness = Harness::new();
        let cases = [
            ("/login?next=/members", "/members"),
            ("/login", "/"),
            ("/login?next=/%09/evil.example", "/"),
            ("/login?next=//evil", "/"),
            ("/login?next=/%2F%2Fevil", "/"),
            ("/login?next=https://evil", "/"),
            ("/login?next=javascript:alert(1)", "/"),
        ];
        for (uri, expected) in cases {
            let response = harness.send_as_owner(Request::get(uri)).await;
            assert_eq!(response.status(), StatusCode::SEE_OTHER, "{uri}");
            assert_eq!(location(&response), expected, "{uri}");
        }
    }

    #[tokio::test]
    async fn security_headers_are_set_on_every_kind_of_response() {
        let harness = Harness::new();
        let responses = [
            harness.send(Request::get("/login")).await,
            harness.send(Request::get("/members")).await,
            harness.send(Request::get("/assets/x.js")).await,
            harness.send(Request::get("/api/ping")).await,
            harness.send_as_owner(Request::get("/")).await,
            harness
                .send_as_owner(Request::get("/api/doesnotexist"))
                .await,
        ];
        for response in &responses {
            let headers: &HeaderMap = response.headers();
            assert_eq!(headers[header::CONTENT_SECURITY_POLICY], CSP);
            assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
            assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        }
    }
}
