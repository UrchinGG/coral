use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRef, Path, State};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use clients::ExtensionError;
use clients::view::ExtensionView;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::audit;
use crate::auth::AdminActor;
use crate::error::AdminError;
use crate::extensions::{self, ExtensionSummary};
use crate::state::AppState;

const AUDIT_ACTION: &str = "extension";

#[derive(Clone)]
pub struct ExtensionHost {
    extensions: Arc<[Arc<dyn clients::Extension>]>,
    pool: PgPool,
}

#[derive(Serialize)]
struct ActionResponse {
    message: String,
    view: ExtensionView,
}

impl FromRef<AppState> for ExtensionHost {
    fn from_ref(state: &AppState) -> Self {
        Self {
            extensions: state.extensions.clone(),
            pool: state.db.pool().clone(),
        }
    }
}

impl ExtensionHost {
    fn find(&self, id: &str) -> Result<&dyn clients::Extension, AdminError> {
        self.extensions
            .iter()
            .find(|extension| extension.id() == id)
            .map(Arc::as_ref)
            .ok_or_else(|| AdminError::NotFound(format!("no extension is named {id}")))
    }
}

pub fn router<S>() -> Router<S>
where
    ExtensionHost: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(list))
        .route("/{id}", get(view))
        .route("/{id}/{action}", post(act))
}

async fn list(State(host): State<ExtensionHost>) -> Json<Vec<ExtensionSummary>> {
    Json(extensions::summaries(&host.extensions))
}

async fn view(
    State(host): State<ExtensionHost>,
    Path(id): Path<String>,
) -> Result<Json<ExtensionView>, AdminError> {
    let view = host
        .find(&id)?
        .view()
        .await
        .map_err(|error| extension_failure(&id, error))?;
    Ok(Json(view))
}

async fn act(
    State(host): State<ExtensionHost>,
    Extension(actor): Extension<AdminActor>,
    Path((id, action)): Path<(String, String)>,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<ActionResponse>, AdminError> {
    let Json(input) = input?;
    let outcome = host
        .find(&id)?
        .act(&action, input)
        .await
        .map_err(|error| extension_failure(&id, error))?;
    let target = format!("{id}/{action}");
    audit::record(
        &host.pool,
        actor.discord_id,
        AUDIT_ACTION,
        &target,
        &outcome.audit,
    )
    .await;
    Ok(Json(ActionResponse {
        message: outcome.message,
        view: outcome.view,
    }))
}

fn extension_failure(id: &str, error: ExtensionError) -> AdminError {
    match error {
        ExtensionError::Rejected(reason) => AdminError::Rejected(reason),
        ExtensionError::UnknownAction(_) => {
            AdminError::NotFound(format!("extension {id}: {error}"))
        }
        ExtensionError::Failed(reason) => AdminError::Upstream(format!("extension {id}: {reason}")),
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use chrono::Utc;
    use clients::{ActionOutcome, ExtensionFlag};
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    const BODY_LIMIT: usize = 1 << 16;
    const UNREACHABLE_DATABASE_URL: &str = "postgres://coral@127.0.0.1:9/unused";

    struct FakeExtension;

    struct BrokenExtension;

    #[async_trait]
    impl clients::Extension for FakeExtension {
        fn id(&self) -> &'static str {
            "fake"
        }

        fn title(&self) -> &'static str {
            "Fake"
        }

        async fn view(&self) -> Result<ExtensionView, ExtensionError> {
            Ok(fake_view("Fake"))
        }

        async fn act(&self, action: &str, input: Value) -> Result<ActionOutcome, ExtensionError> {
            match action {
                "echo" => Ok(ActionOutcome {
                    message: format!("echoed {}", input["word"]),
                    view: fake_view("Echoed"),
                    audit: json!({ "word": input["word"] }),
                }),
                "refuse" => Err(ExtensionError::Rejected("that word is not allowed".into())),
                "break" => Err(ExtensionError::Failed("the backing store is down".into())),
                other => Err(ExtensionError::UnknownAction(other.into())),
            }
        }

        async fn flags(&self) -> Result<Vec<ExtensionFlag>, ExtensionError> {
            Ok(Vec::new())
        }
    }

    #[async_trait]
    impl clients::Extension for BrokenExtension {
        fn id(&self) -> &'static str {
            "broken"
        }

        fn title(&self) -> &'static str {
            "Broken"
        }

        async fn view(&self) -> Result<ExtensionView, ExtensionError> {
            Err(ExtensionError::Failed("view source unreachable".into()))
        }

        async fn act(&self, action: &str, _input: Value) -> Result<ActionOutcome, ExtensionError> {
            Err(ExtensionError::UnknownAction(action.into()))
        }

        async fn flags(&self) -> Result<Vec<ExtensionFlag>, ExtensionError> {
            Ok(Vec::new())
        }
    }

    fn fake_view(title: &str) -> ExtensionView {
        ExtensionView {
            title: title.into(),
            subtitle: None,
            sections: Vec::new(),
        }
    }

    fn host_with(pool: PgPool) -> ExtensionHost {
        let extensions: Vec<Arc<dyn clients::Extension>> =
            vec![Arc::new(FakeExtension), Arc::new(BrokenExtension)];
        ExtensionHost {
            extensions: extensions.into(),
            pool,
        }
    }

    fn offline_host() -> ExtensionHost {
        host_with(
            PgPoolOptions::new()
                .connect_lazy(UNREACHABLE_DATABASE_URL)
                .unwrap(),
        )
    }

    async fn database_host() -> Option<ExtensionHost> {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL").ok()?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()?;
        Some(host_with(pool))
    }

    async fn send(host: &ExtensionHost, request: Request<Body>) -> (StatusCode, Value) {
        let app = Router::new()
            .nest("/api/extensions", router())
            .with_state(host.clone());
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), BODY_LIMIT).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    fn get_request(path: &str) -> Request<Body> {
        Request::get(path).body(Body::empty()).unwrap()
    }

    fn action_request(path: &str, actor: i64, input: Value) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .extension(AdminActor { discord_id: actor })
            .body(Body::from(input.to_string()))
            .unwrap()
    }

    async fn take_audit_rows(pool: &PgPool, actor: i64) -> Vec<(String, String, Value)> {
        sqlx::query_as(
            "DELETE FROM admin_actions WHERE actor = $1 RETURNING action, target, details",
        )
        .bind(actor)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    fn error_code(body: &Value) -> &str {
        body["error"]["code"].as_str().unwrap()
    }

    #[test]
    fn extension_errors_map_to_the_documented_statuses() {
        let cases = [
            (ExtensionError::Rejected("no".into()), 422, "rejected"),
            (ExtensionError::UnknownAction("x".into()), 404, "not_found"),
            (ExtensionError::Failed("down".into()), 502, "upstream"),
        ];
        for (error, status, code) in cases {
            let failure = extension_failure("fake", error);
            assert_eq!(failure.status().as_u16(), status, "{code}");
            assert_eq!(failure.code(), code);
        }
    }

    #[tokio::test]
    async fn list_names_every_extension_in_order() {
        let (status, body) = send(&offline_host(), get_request("/api/extensions")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!([{ "id": "fake", "title": "Fake" }, { "id": "broken", "title": "Broken" }])
        );
    }

    #[tokio::test]
    async fn view_returns_the_extension_view() {
        let (status, body) = send(&offline_host(), get_request("/api/extensions/fake")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({ "title": "Fake", "subtitle": null, "sections": [] })
        );
    }

    #[tokio::test]
    async fn view_of_an_unknown_extension_is_not_found() {
        let (status, body) = send(&offline_host(), get_request("/api/extensions/missing")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error_code(&body), "not_found");
    }

    #[tokio::test]
    async fn failing_view_is_a_bad_gateway_with_its_reason() {
        let (status, body) = send(&offline_host(), get_request("/api/extensions/broken")).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            body,
            json!({ "error": { "code": "upstream", "message": "extension broken: view source unreachable" } })
        );
    }

    #[tokio::test]
    async fn rejected_action_is_unprocessable_with_its_reason() {
        let request = action_request("/api/extensions/fake/refuse", 1, json!({}));
        let (status, body) = send(&offline_host(), request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body,
            json!({ "error": { "code": "rejected", "message": "that word is not allowed" } })
        );
    }

    #[tokio::test]
    async fn failed_action_is_a_bad_gateway() {
        let request = action_request("/api/extensions/fake/break", 1, json!({}));
        let (status, body) = send(&offline_host(), request).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            body["error"]["message"],
            "extension fake: the backing store is down"
        );
    }

    #[tokio::test]
    async fn malformed_action_body_is_a_json_bad_request() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/extensions/fake/echo")
            .header(header::CONTENT_TYPE, "application/json")
            .extension(AdminActor { discord_id: 1 })
            .body(Body::from("{"))
            .unwrap();
        let (status, body) = send(&offline_host(), request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error_code(&body), "bad_request");
    }

    #[tokio::test]
    async fn unknown_action_or_extension_is_not_found() {
        let host = offline_host();
        for path in ["/api/extensions/fake/nope", "/api/extensions/missing/echo"] {
            let (status, body) = send(&host, action_request(path, 1, json!({}))).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(error_code(&body), "not_found", "{path}");
        }
    }

    #[tokio::test]
    async fn only_successful_actions_are_audited() {
        let Some(host) = database_host().await else {
            return;
        };
        let actor = Utc::now().timestamp_nanos_opt().unwrap();
        let echo = action_request("/api/extensions/fake/echo", actor, json!({ "word": "hi" }));
        let refuse = action_request("/api/extensions/fake/refuse", actor, json!({}));

        let (status, body) = send(&host, echo).await;
        send(&host, refuse).await;
        let audited = take_audit_rows(&host.pool, actor).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({ "message": "echoed \"hi\"", "view": { "title": "Echoed", "subtitle": null, "sections": [] } })
        );
        assert_eq!(
            audited,
            vec![(
                AUDIT_ACTION.to_string(),
                "fake/echo".to_string(),
                json!({ "word": "hi" })
            )]
        );
    }
}
