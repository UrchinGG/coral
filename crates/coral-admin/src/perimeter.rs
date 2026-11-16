use std::sync::Arc;

use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::Redirect;
use axum::routing::get;

use crate::auth::{self, AuthConfig};
use crate::boot::BootSource;
use crate::{layers, permissions, routes, security};

/// Where the dashboard is served on the Urchin site; Caddy forwards this path to coral-admin.
pub const MOUNT: &str = "/dashboard";

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
        .layer(from_fn(permissions::authorize))
        .layer(from_fn_with_state(
            auth_config.clone(),
            auth::require_signed_in,
        ))
        .layer(from_fn(security::no_store));
    let ui = routes::ui_router(boot).layer(from_fn_with_state(
        auth_config,
        auth::require_signed_in_page,
    ));
    let dashboard = Router::new().nest("/api", api).merge(ui);
    let app = Router::new()
        .route(
            &format!("{MOUNT}/"),
            get(|| async { Redirect::permanent(MOUNT) }),
        )
        .nest(MOUNT, dashboard)
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
    use futures_util::stream;
    use tower::ServiceExt;
    use url::Url;

    use super::*;
    use crate::coverage::{Coverage, CoverageCache};
    use crate::session::SESSION_COOKIE;
    use crate::session::tests::MemorySessions;

    const OWNER: i64 = 111;
    const STRANGER: i64 = 222;
    const HELPER: i64 = 333;
    const BASE: &str = "https://admin.example";
    const BODY_LIMIT: usize = 1 << 20;
    const BUILD: &str = "test-build";
    const HOME_PAGE: &str = "/urchin";
    const LANDING: &str = "/dashboard/account";
    const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https://cdn.discordapp.com https://mc-heads.net; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

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
                ranks: Arc::new(FixedRanks),
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
            let path = request.uri_ref().unwrap().to_string();
            let target = match path.as_str() {
                "/" => MOUNT.to_string(),
                _ => format!("{MOUNT}{path}"),
            };
            let request = request.uri(target);
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

    struct FixedRanks;

    #[async_trait::async_trait]
    impl auth::StaffRanks for FixedRanks {
        async fn rank(&self, discord_id: i64) -> database::AccessRank {
            match discord_id {
                HELPER => database::AccessRank::Helper,
                _ => database::AccessRank::Default,
            }
        }
    }

    fn stub_api() -> Router {
        let members = Router::new().route("/{id}", get(|| async { "member" }));
        let accounts = Router::new().route("/{id}/network", get(|| async { "network" }));
        Router::new()
            .route("/session", get(|| async { "boot" }))
            .nest("/accounts", accounts)
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

    #[tokio::test]
    async fn unauthenticated_page_goes_to_the_site_sign_in_and_returns_there() {
        let harness = Harness::new();
        let cases = [
            (
                "/members?q=1",
                "https://admin.example/dashboard/members?q=1",
            ),
            ("/", "https://admin.example/dashboard"),
        ];
        for (path, return_to) in cases {
            let response = harness.send(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::SEE_OTHER, "{path}");
            let sign_in = location(&response);
            assert!(
                sign_in.starts_with("https://urchin.gg/auth/discord?"),
                "{sign_in}"
            );
            assert_eq!(query_param(sign_in, "return"), return_to, "{path}");
        }
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
    async fn owner_gets_each_page_without_caching() {
        let harness = Harness::new();
        for path in [
            HOME_PAGE,
            "/urchin/members?id=12",
            "/starfish/plugins?slug=x",
        ] {
            let response = harness.send_as_owner(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert!(header_value(&response, header::CONTENT_TYPE).starts_with("text/html"));
            assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
            assert!(body_text(response).await.contains("<main id=\"content\">"));
        }
    }

    #[tokio::test]
    async fn owner_root_redirects_home_and_unknown_pages_are_missing() {
        let harness = Harness::new();
        let root = harness.send_as_owner(Request::get("/")).await;
        assert_eq!(root.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&root), LANDING);
        let missing = harness.send_as_owner(Request::get("/members/12")).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn owner_index_carries_the_boot_payload_for_the_signed_in_owner() {
        let harness = Harness::new();
        let index = body_text(
            harness
                .send_as_owner(Request::get("/urchin/players?q=x"))
                .await,
        )
        .await;
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
        let index = body_text(harness.send_as_owner(Request::get(HOME_PAGE)).await).await;
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
    async fn a_member_without_a_staff_rank_gets_pages_and_the_session_only() {
        let harness = Harness::new();
        let cookie = harness.session_cookie(STRANGER);
        let session = harness
            .send(Request::get("/api/session").header(header::COOKIE, &cookie))
            .await;
        assert_eq!(session.status(), StatusCode::OK);
        let api = harness
            .send(Request::get("/api/ping").header(header::COOKIE, &cookie))
            .await;
        assert_eq!(api.status(), StatusCode::FORBIDDEN);
        let page = harness
            .send(Request::get(HOME_PAGE).header(header::COOKIE, &cookie))
            .await;
        assert_eq!(page.status(), StatusCode::OK);
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
    async fn security_headers_are_set_on_every_kind_of_response() {
        let harness = Harness::new();
        let responses = [
            harness.send(Request::get("/members")).await,
            harness.send(Request::get("/assets/x.js")).await,
            harness.send(Request::get("/api/ping")).await,
            harness.send_as_owner(Request::get(HOME_PAGE)).await,
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

    #[tokio::test]
    async fn a_helper_reaches_staff_routes_but_not_owner_ones() {
        let harness = Harness::new();
        let helper = harness.session_cookie(HELPER);
        let session = harness
            .send(Request::get("/api/session").header(header::COOKIE, &helper))
            .await;
        assert_eq!(session.status(), StatusCode::OK);
        for path in ["/api/accounts/5/network", "/api/ping"] {
            let response = harness
                .send(Request::get(path).header(header::COOKIE, &helper))
                .await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
            assert_eq!(error_code(response).await, "forbidden", "{path}");
        }
        let owner = harness
            .send_as_owner(Request::get("/api/accounts/5/network"))
            .await;
        assert_eq!(owner.status(), StatusCode::OK);
    }
}
