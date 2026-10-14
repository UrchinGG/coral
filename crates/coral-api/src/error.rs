use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

use reqlog::Cause;

const RATE_LIMITED: &str = "Rate limited";
const UPSTREAM_FAILED: &str = "Upstream request failed";
const INTERNAL_ERROR: &str = "Internal error";

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    Conflict(String),
    ServiceUnavailable(String),
    RateLimited,
    UpstreamLimited,
    Upstream(String),
    Internal(String),
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ErrorResponse {
    pub error: String,
}

impl ApiError {
    fn cause(&self) -> Cause {
        match self {
            Self::NotFound(_) => Cause::NotFound,
            Self::BadRequest(_) | Self::Conflict(_) => Cause::Validation,
            Self::Unauthorized(_) | Self::Forbidden(_) => Cause::Auth,
            Self::RateLimited => Cause::CallerLimit,
            Self::UpstreamLimited => Cause::UpstreamLimit,
            Self::Upstream(_) => Cause::UpstreamError,
            Self::Internal(_) | Self::ServiceUnavailable(_) => Cause::Internal,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        reqlog::record_cause(self.cause());
        let (status, message) = match self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg),
            Self::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            Self::RateLimited | Self::UpstreamLimited => {
                (StatusCode::TOO_MANY_REQUESTS, RATE_LIMITED.into())
            }
            Self::Upstream(detail) => {
                report_server_error(&detail);
                (StatusCode::BAD_GATEWAY, UPSTREAM_FAILED.into())
            }
            Self::Internal(detail) => {
                report_server_error(&detail);
                (StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR.into())
            }
        };
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

impl From<clients::ClientError> for ApiError {
    fn from(err: clients::ClientError) -> Self {
        match err {
            clients::ClientError::PlayerNotFound(p) => {
                Self::NotFound(format!("Player not found: {p}"))
            }
            clients::ClientError::RateLimited => Self::UpstreamLimited,
            clients::ClientError::InvalidUuid(u) => Self::BadRequest(format!("Invalid UUID: {u}")),
            other => Self::Upstream(other.to_string()),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(format!("Database error: {err}"))
    }
}

pub fn report_server_error(detail: &str) {
    tracing::error!("{detail}");
    reqlog::record_server_error(detail);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqlog::RequestContext;

    use super::*;

    async fn render(error: ApiError) -> (StatusCode, String, reqlog::Recorded) {
        let context = Arc::new(RequestContext::default());
        let response = reqlog::scope(context.clone(), async { error.into_response() }).await;
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            String::from_utf8(body.to_vec()).unwrap(),
            context.take(),
        )
    }

    #[tokio::test]
    async fn internal_errors_return_fixed_text_and_record_the_detail() {
        let (status, body, recorded) = render(ApiError::Internal(
            "Database error: relation missing".into(),
        ))
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, r#"{"error":"Internal error"}"#);
        assert_eq!(
            recorded.server_error.as_deref(),
            Some("Database error: relation missing")
        );
        assert_eq!(recorded.cause, Some(Cause::Internal));
    }

    #[tokio::test]
    async fn upstream_errors_return_fixed_text_and_record_the_detail() {
        let error = ApiError::from(clients::ClientError::HypixelApi(
            "unexpected payload".into(),
        ));
        let (status, body, recorded) = render(error).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body, r#"{"error":"Upstream request failed"}"#);
        assert_eq!(
            recorded.server_error.as_deref(),
            Some("Hypixel API error: unexpected payload")
        );
        assert_eq!(recorded.cause, Some(Cause::UpstreamError));
    }

    #[tokio::test]
    async fn upstream_limits_keep_the_public_text_but_not_the_cause() {
        let (status, body, recorded) =
            render(ApiError::from(clients::ClientError::RateLimited)).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body, r#"{"error":"Rate limited"}"#);
        assert_eq!(recorded.cause, Some(Cause::UpstreamLimit));

        let (_, body, recorded) = render(ApiError::RateLimited).await;
        assert_eq!(body, r#"{"error":"Rate limited"}"#);
        assert_eq!(recorded.cause, Some(Cause::CallerLimit));
    }

    #[tokio::test]
    async fn client_errors_keep_their_message() {
        let (status, body, recorded) =
            render(ApiError::NotFound("Player not found: x".into())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, r#"{"error":"Player not found: x"}"#);
        assert_eq!(recorded.server_error, None);
        assert_eq!(recorded.cause, Some(Cause::NotFound));
    }
}
