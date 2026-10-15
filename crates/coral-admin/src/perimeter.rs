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
    use std::collections::{HashMap, HashSet};
    use std::convert::Infallible;

    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderMap, HeaderName, Request, Response, StatusCode, header, request};
    use axum::response::sse::{Event, Sse};
    use axum::routing::{get, post};
    use axum::{Form, Json};
    use chrono::Utc;
    use futures_util::stream;
    use serde_json::json;
    use tokio::net::TcpListener;
    use tower::ServiceExt;
    use url::Url;

    use super::*;
    use crate::auth::DiscordOAuth;
    use crate::coverage::{Coverage, CoverageCache};
    use crate::epoch::SessionEpoch;
    use crate::epoch::tests::{MemoryEpochStore, StoreHealth};
    use crate::session::{
        AuthKeys, OAUTH_STATE_COOKIE, OAuthState, SESSION_COOKIE, SESSION_TTL, Session,
    };

    const OWNER: i64 = 111;
    const STRANGER: i64 = 222;
    const BASE: &str = "https://admin.example";
    const BODY_LIMIT: usize = 1 << 20;
    const LOGIN_PAGE_LIMIT: usize = 2048;
    const DISCORD_CODE: &str = "granted-code";
    const DISCORD_ACCESS_TOKEN: &str = "access-token";
    const BUILD: &str = "test-build";
    const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

    struct Harness {
        auth: Arc<AuthConfig>,
        store: Arc<MemoryEpochStore>,
    }

    impl Harness {
        async fn new() -> Self {
            let base_url = Url::parse(BASE).unwrap();
            let discord = DiscordOAuth::new("client".into(), "secret".into(), &base_url).unwrap();
            Self::with_discord(base_url, discord).await
        }

        async fn signing_in_as(discord_id: i64) -> Self {
            let base_url = Url::parse(BASE).unwrap();
            let api_url = spawn_discord_stub(discord_id).await;
            let discord =
                DiscordOAuth::with_api("client".into(), "secret".into(), &base_url, &api_url)
                    .unwrap();
            Self::with_discord(base_url, discord).await
        }

        async fn with_discord(base_url: Url, discord: DiscordOAuth) -> Self {
            let store = Arc::new(MemoryEpochStore::default());
            let auth = AuthConfig {
                owner_ids: HashSet::from([OWNER]),
                discord,
                keys: AuthKeys::derive(&[7u8; 32]),
                epoch: Arc::new(SessionEpoch::load(store.clone()).await.unwrap()),
                base_url,
            };
            Self {
                auth: Arc::new(auth),
                store,
            }
        }

        fn session_cookie(&self, discord_id: i64) -> String {
            let session = Session::start(discord_id, self.auth.epoch.current());
            format!("{SESSION_COOKIE}={}", session.seal(&self.auth.keys.session))
        }

        fn owner_cookie(&self) -> String {
            self.session_cookie(OWNER)
        }

        fn state_cookie(&self, state: &OAuthState) -> String {
            format!(
                "{OAUTH_STATE_COOKIE}={}",
                state.seal(&self.auth.keys.oauth_state)
            )
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

    async fn spawn_discord_stub(discord_id: i64) -> Url {
        let token = post(|Form(grant): Form<HashMap<String, String>>| async move {
            if grant.get("code").map(String::as_str) != Some(DISCORD_CODE) {
                return Err(StatusCode::BAD_REQUEST);
            }
            Ok(Json(json!({ "access_token": DISCORD_ACCESS_TOKEN })))
        });
        let user = get(move |headers: HeaderMap| async move {
            let bearer = format!("Bearer {DISCORD_ACCESS_TOKEN}");
            if headers
                .get(header::AUTHORIZATION)
                .is_none_or(|value| value != bearer.as_str())
            {
                return Err(StatusCode::UNAUTHORIZED);
            }
            Ok(Json(json!({ "id": discord_id.to_string() })))
        });
        let api = Router::new()
            .route("/oauth2/token", token)
            .route("/users/@me", user);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, api).await.unwrap() });
        Url::parse(&format!("http://{address}/")).unwrap()
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

    fn cookie_parts<'a>(cookie: &'a str, name: &str) -> (&'a str, &'a str) {
        cookie
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
            .and_then(|rest| rest.split_once("; "))
            .unwrap()
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
        let harness = Harness::new().await;
        let response = harness.send(Request::get("/members?q=1")).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&response), "/login?next=%2Fmembers%3Fq%3D1");

        let root = harness.send(Request::get("/")).await;
        assert_eq!(root.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&root), "/login?next=%2F");
    }

    #[tokio::test]
    async fn unauthenticated_assets_get_an_empty_401() {
        let harness = Harness::new().await;
        for path in ["/assets/index.js", "/assets/nope.css"] {
            let response = harness.send(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(body_text(response).await, "", "{path}");
        }
    }

    #[tokio::test]
    async fn unauthenticated_non_navigation_page_request_gets_401() {
        let harness = Harness::new().await;
        let response = harness.send(Request::post("/members")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unauthenticated_api_gets_json_401_for_every_path() {
        let harness = Harness::new().await;
        for path in ["/api/ping", "/api/members/5", "/api/doesnotexist", "/api"] {
            let response = harness.send(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
            assert_eq!(error_code(response).await, "unauthorized", "{path}");
        }
    }

    #[tokio::test]
    async fn unauthenticated_api_write_gets_401_before_the_origin_check() {
        let harness = Harness::new().await;
        let response = harness.send(Request::post("/api/ping")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn owner_reaches_api_handlers() {
        let harness = Harness::new().await;
        let ping = harness.send_as_owner(Request::get("/api/ping")).await;
        assert_eq!(ping.status(), StatusCode::OK);
        assert_eq!(header_value(&ping, header::CACHE_CONTROL), "no-store");
        assert_eq!(body_text(ping).await, "pong");

        let member = harness.send_as_owner(Request::get("/api/members/5")).await;
        assert_eq!(body_text(member).await, "member");
    }

    #[tokio::test]
    async fn owner_event_streams_are_not_stored_either() {
        let harness = Harness::new().await;
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
        let harness = Harness::new().await;
        for path in ["/api/doesnotexist", "/api/members", "/api"] {
            let response = harness.send_as_owner(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
            assert_eq!(error_code(response).await, "not_found", "{path}");
        }
    }

    #[tokio::test]
    async fn owner_writes_need_same_origin_fetch_metadata_or_origin() {
        let harness = Harness::new().await;
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
        let harness = Harness::new().await;
        let request = Request::post("/api/ping").header("sec-fetch-site", "cross-site");
        let response = harness.send_as_owner(request).await;
        assert_eq!(error_code(response).await, "cross_origin");
    }

    #[tokio::test]
    async fn owner_gets_index_html_without_caching_for_spa_paths() {
        let harness = Harness::new().await;
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
        let harness = Harness::new().await;
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
        let harness = Harness::new().await;
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
        let harness = Harness::new().await;
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
        let harness = Harness::new().await;
        let response = harness.send_as_owner(Request::get("/assets/nope.js")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_text(response).await, "");
    }

    #[tokio::test]
    async fn brotli_clients_get_the_asset_precompressed_at_build_time() {
        let harness = Harness::new().await;
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
        let harness = Harness::new().await;
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
    async fn expired_or_unprefixed_session_cookies_are_rejected() {
        let harness = Harness::new().await;
        let expired = Session {
            discord_id: OWNER,
            expires_at: Utc::now().timestamp() - 1,
            epoch: harness.auth.epoch.current(),
        };
        let sealed = expired.seal(&harness.auth.keys.session);
        let valid = Session::start(OWNER, 0).seal(&harness.auth.keys.session);
        for cookie in [
            format!("{SESSION_COOKIE}={sealed}"),
            format!("coral_admin_session={valid}"),
        ] {
            let request = Request::get("/api/ping").header(header::COOKIE, cookie);
            assert_eq!(
                harness.send(request).await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }

    #[tokio::test]
    async fn session_from_an_older_epoch_is_rejected_after_refresh() {
        let harness = Harness::new().await;
        let cookie = harness.owner_cookie();
        harness.store.set(3);
        harness.auth.epoch.refresh().await.unwrap();
        let request = Request::get("/api/ping").header(header::COOKIE, cookie);
        assert_eq!(
            harness.send(request).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let fresh = harness.send_as_owner(Request::get("/api/ping")).await;
        assert_eq!(fresh.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn logout_revokes_every_session_and_clears_the_cookie() {
        let harness = Harness::new().await;
        let first_browser = harness.owner_cookie();
        let second_browser = harness.owner_cookie();
        let logout = Request::post("/auth/logout")
            .header(header::COOKIE, &first_browser)
            .header("sec-fetch-site", "same-origin");
        let response = harness.send(logout).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&response), "/login");
        assert_eq!(
            set_cookies(&response),
            [format!(
                "{SESSION_COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
            )]
        );
        assert_eq!(harness.auth.epoch.current(), 1);
        for cookie in [first_browser, second_browser] {
            let request = Request::get("/api/ping").header(header::COOKIE, cookie);
            assert_eq!(
                harness.send(request).await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }

    #[tokio::test]
    async fn logout_without_a_valid_session_clears_the_cookie_and_revokes_nothing() {
        let harness = Harness::new().await;
        let stale = harness.owner_cookie();
        harness.store.set(2);
        harness.auth.epoch.refresh().await.unwrap();
        for cookie in [None, Some(stale), Some(harness.session_cookie(STRANGER))] {
            let mut request = Request::post("/auth/logout").header("sec-fetch-site", "same-origin");
            if let Some(cookie) = &cookie {
                request = request.header(header::COOKIE, cookie);
            }
            let response = harness.send(request).await;
            assert_eq!(response.status(), StatusCode::SEE_OTHER, "{cookie:?}");
            assert_eq!(location(&response), "/login", "{cookie:?}");
            assert_eq!(
                set_cookies(&response),
                [format!(
                    "{SESSION_COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
                )]
            );
        }
        assert_eq!(harness.auth.epoch.current(), 2);
    }

    #[tokio::test]
    async fn failed_revocation_signs_out_this_browser_and_says_so() {
        let harness = Harness::new().await;
        let owner = harness.owner_cookie();
        harness.store.set_health(StoreHealth::Failing);
        let logout = Request::post("/auth/logout")
            .header(header::COOKIE, &owner)
            .header("sec-fetch-site", "same-origin");
        let response = harness.send(logout).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&response), "/login?error=not_revoked");
        assert_eq!(
            set_cookies(&response),
            [format!(
                "{SESSION_COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
            )]
        );
        let notice = body_text(harness.send(Request::get(location(&response))).await).await;
        assert!(notice.contains("other sessions could not be revoked"));
    }

    #[tokio::test]
    async fn logout_requires_same_origin_and_a_post() {
        let harness = Harness::new().await;
        let cross_site = harness
            .send_as_owner(Request::post("/auth/logout").header("sec-fetch-site", "cross-site"))
            .await;
        assert_eq!(cross_site.status(), StatusCode::FORBIDDEN);
        let by_link = harness.send_as_owner(Request::get("/auth/logout")).await;
        assert_eq!(by_link.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(harness.auth.epoch.current(), 0);
    }

    #[tokio::test]
    async fn login_page_is_small_script_free_html() {
        let harness = Harness::new().await;
        let response = harness.send(Request::get("/login")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(header_value(&response, header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(header_value(&response, header::CACHE_CONTROL), "no-store");
        let page = body_text(response).await;
        assert!(page.len() <= LOGIN_PAGE_LIMIT, "{} bytes", page.len());
        assert!(!page.contains("<script"));
        assert!(page.contains("href=\"/auth/login?next=%2F\""));
        assert!(!page.contains("class=\"notice\""));
    }

    #[tokio::test]
    async fn login_page_links_with_a_validated_next() {
        let harness = Harness::new().await;
        let cases = [
            ("/login?next=%2Fmembers%3Fq%3D1", "%2Fmembers%3Fq%3D1"),
            ("/login?next=//evil", "%2F"),
            ("/login?next=/%09/evil.example", "%2F"),
            (
                "/login?next=%22%3E%3Cscript%3E",
                "%2F%2522%253E%253Cscript%253E",
            ),
        ];
        for (uri, encoded_next) in cases {
            let page = body_text(harness.send(Request::get(uri)).await).await;
            let link = format!("href=\"/auth/login?next={encoded_next}\"");
            assert!(page.contains(&link), "{uri}: {page}");
        }
    }

    #[tokio::test]
    async fn login_page_treats_a_repeated_next_as_unsafe() {
        let harness = Harness::new().await;
        let response = harness
            .send(Request::get("/login?next=/members&next=/players"))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            body_text(response)
                .await
                .contains("href=\"/auth/login?next=%2F\"")
        );
    }

    #[tokio::test]
    async fn login_page_shows_only_known_notices() {
        let harness = Harness::new().await;
        let denied = body_text(harness.send(Request::get("/login?error=denied")).await).await;
        assert!(denied.contains("That Discord account is not an owner."));
        let unknown = harness.send(Request::get("/login?error=%3Cb%3Ehi")).await;
        assert_eq!(unknown.status(), StatusCode::OK);
        let unknown = body_text(unknown).await;
        assert!(!unknown.contains("class=\"notice\""));
        assert!(!unknown.contains("<b>hi"));
    }

    #[tokio::test]
    async fn signed_in_owner_on_login_page_is_sent_to_a_safe_next() {
        let harness = Harness::new().await;
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
    async fn start_login_sets_a_signed_state_cookie_and_redirects_to_discord() {
        let harness = Harness::new().await;
        let response = harness
            .send(Request::get("/auth/login?next=%2Fmembers"))
            .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let authorize = location(&response);
        assert!(authorize.starts_with("https://discord.com/oauth2/authorize?"));
        assert_eq!(
            query_param(authorize, "redirect_uri"),
            "https://admin.example/auth/callback"
        );
        assert_eq!(query_param(authorize, "scope"), "identify");
        let [cookie] = set_cookies(&response).try_into().unwrap();
        let (token, attributes) = cookie_parts(&cookie, OAUTH_STATE_COOKIE);
        assert_eq!(
            attributes,
            "HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600"
        );
        let state = OAuthState::open(token, &harness.auth.keys.oauth_state).unwrap();
        assert_eq!(state.next, "/members");
        assert!(state.matches(&query_param(authorize, "state")));
    }

    #[tokio::test]
    async fn start_login_treats_a_repeated_next_as_unsafe() {
        let harness = Harness::new().await;
        let response = harness
            .send(Request::get("/auth/login?next=/members&next=/players"))
            .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let [cookie] = set_cookies(&response).try_into().unwrap();
        let (token, _) = cookie_parts(&cookie, OAUTH_STATE_COOKIE);
        let state = OAuthState::open(token, &harness.auth.keys.oauth_state).unwrap();
        assert_eq!(state.next, "/");
    }

    #[tokio::test]
    async fn callback_rejects_bad_state_and_always_clears_the_state_cookie() {
        let harness = Harness::new().await;
        let state = OAuthState::start("/members".into());
        let expired = OAuthState {
            expires_at: Utc::now().timestamp() - 1,
            ..OAuthState::start("/".into())
        };
        let cases = [
            (format!("/auth/callback?code=c&state={}", state.nonce), None),
            (
                "/auth/callback?code=c&state=other".to_string(),
                Some(harness.state_cookie(&state)),
            ),
            (
                "/auth/callback?code=c".to_string(),
                Some(harness.state_cookie(&state)),
            ),
            (
                format!("/auth/callback?code=c&state={}", expired.nonce),
                Some(harness.state_cookie(&expired)),
            ),
            (
                format!("/auth/callback?error=access_denied&state={}", state.nonce),
                Some(harness.state_cookie(&state)),
            ),
            (
                format!("/auth/callback?code=c&state={0}&state={0}", state.nonce),
                Some(harness.state_cookie(&state)),
            ),
        ];
        for (uri, cookie) in cases {
            let mut request = Request::get(uri.as_str());
            if let Some(cookie) = cookie {
                request = request.header(header::COOKIE, cookie);
            }
            let response = harness.send(request).await;
            assert_eq!(response.status(), StatusCode::SEE_OTHER, "{uri}");
            assert_eq!(location(&response), "/login?error=failed", "{uri}");
            assert_eq!(
                set_cookies(&response),
                [format!(
                    "{OAUTH_STATE_COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
                )],
                "{uri}"
            );
        }
    }

    #[tokio::test]
    async fn owner_callback_signs_in_at_the_current_epoch_and_returns_to_next() {
        let harness = Harness::signing_in_as(OWNER).await;
        harness.store.set(4);
        harness.auth.epoch.refresh().await.unwrap();
        let state = OAuthState::start("/players/abc?tab=tags".into());
        let callback = format!("/auth/callback?code={DISCORD_CODE}&state={}", state.nonce);
        let request = Request::get(callback).header(header::COOKIE, harness.state_cookie(&state));
        let response = harness.send(request).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&response), "/players/abc?tab=tags");

        let [clear_state, session_cookie] = set_cookies(&response).try_into().unwrap();
        assert_eq!(
            clear_state,
            format!("{OAUTH_STATE_COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0")
        );
        let (token, attributes) = cookie_parts(&session_cookie, SESSION_COOKIE);
        assert_eq!(
            attributes,
            format!(
                "HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
                SESSION_TTL.num_seconds()
            )
        );
        let session = Session::open(token, &harness.auth.keys.session).unwrap();
        assert_eq!((session.discord_id, session.epoch), (OWNER, 4));

        let api =
            Request::get("/api/ping").header(header::COOKIE, format!("{SESSION_COOKIE}={token}"));
        assert_eq!(harness.send(api).await.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_owner_callback_is_denied_without_a_session() {
        let harness = Harness::signing_in_as(STRANGER).await;
        let state = OAuthState::start("/".into());
        let callback = format!("/auth/callback?code={DISCORD_CODE}&state={}", state.nonce);
        let request = Request::get(callback).header(header::COOKIE, harness.state_cookie(&state));
        let response = harness.send(request).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&response), "/login?error=denied");
        assert_eq!(
            set_cookies(&response),
            [format!(
                "{OAUTH_STATE_COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
            )]
        );
    }

    #[tokio::test]
    async fn rejected_code_exchange_fails_the_sign_in() {
        let harness = Harness::signing_in_as(OWNER).await;
        let state = OAuthState::start("/".into());
        let callback = format!("/auth/callback?code=forged&state={}", state.nonce);
        let request = Request::get(callback).header(header::COOKIE, harness.state_cookie(&state));
        let response = harness.send(request).await;
        assert_eq!(location(&response), "/login?error=failed");
        assert_eq!(set_cookies(&response).len(), 1);
    }

    #[tokio::test]
    async fn security_headers_are_set_on_every_kind_of_response() {
        let harness = Harness::new().await;
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
