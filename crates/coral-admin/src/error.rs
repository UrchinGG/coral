use std::fmt::Display;

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

const QUERY_CANCELED: &str = "57014";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdminError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Database(String),
    #[error("{0}")]
    Redis(String),
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    Rejected(String),
    #[error("{0}")]
    Internal(String),
}

pub trait Context<T> {
    fn context(self, what: impl Display) -> Result<T, AdminError>;
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorDetail<'a>,
}

#[derive(Serialize)]
struct ErrorDetail<'a> {
    code: &'static str,
    message: &'a str,
}

impl AdminError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Timeout(_) => "timeout",
            Self::Database(_) => "database",
            Self::Redis(_) => "redis",
            Self::Upstream(_) => "upstream",
            Self::Rejected(_) => "rejected",
            Self::Internal(_) => "internal",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Redis(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Rejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::NotFound(message)
            | Self::BadRequest(message)
            | Self::Timeout(message)
            | Self::Database(message)
            | Self::Redis(message)
            | Self::Upstream(message)
            | Self::Rejected(message)
            | Self::Internal(message) => message,
        }
    }

    pub fn with_context(self, what: impl Display) -> Self {
        let message = format!("{what}: {}", self.message());
        match self {
            Self::NotFound(_) => Self::NotFound(message),
            Self::BadRequest(_) => Self::BadRequest(message),
            Self::Timeout(_) => Self::Timeout(message),
            Self::Database(_) => Self::Database(message),
            Self::Redis(_) => Self::Redis(message),
            Self::Upstream(_) => Self::Upstream(message),
            Self::Rejected(_) => Self::Rejected(message),
            Self::Internal(_) => Self::Internal(message),
        }
    }

    fn log(&self) {
        match self {
            Self::Timeout(_) => tracing::warn!(code = self.code(), "{self}"),
            Self::Database(_) | Self::Redis(_) | Self::Upstream(_) | Self::Internal(_) => {
                tracing::error!(code = self.code(), "{self}")
            }
            Self::NotFound(_) | Self::BadRequest(_) | Self::Rejected(_) => {}
        }
    }
}

pub fn is_statement_timeout(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.code())
        .is_some_and(|code| code == QUERY_CANCELED)
}

impl<T, E: Into<AdminError>> Context<T> for Result<T, E> {
    fn context(self, what: impl Display) -> Result<T, AdminError> {
        self.map_err(|error| error.into().with_context(what))
    }
}

impl From<sqlx::Error> for AdminError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            _ if is_statement_timeout(&error) => Self::Timeout(error.to_string()),
            sqlx::Error::PoolTimedOut => Self::Timeout(error.to_string()),
            _ => Self::Database(error.to_string()),
        }
    }
}

impl From<redis::RedisError> for AdminError {
    fn from(error: redis::RedisError) -> Self {
        Self::Redis(error.to_string())
    }
}

impl From<JsonRejection> for AdminError {
    fn from(rejection: JsonRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<QueryRejection> for AdminError {
    fn from(rejection: QueryRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        self.log();
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.message(),
            },
        };
        (self.status(), Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use serde_json::{Value, json};

    use super::*;

    const BODY_LIMIT: usize = 4096;

    async fn rendered(error: AdminError) -> (StatusCode, Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), BODY_LIMIT).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn errors_render_as_a_json_code_and_message() {
        let (status, body) = rendered(AdminError::NotFound("no such request".into())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body,
            json!({ "error": { "code": "not_found", "message": "no such request" } })
        );
    }

    #[tokio::test]
    async fn every_variant_has_its_own_status_and_code() {
        let cases = [
            (AdminError::BadRequest(String::new()), 400, "bad_request"),
            (AdminError::Timeout(String::new()), 504, "timeout"),
            (AdminError::Database(String::new()), 500, "database"),
            (AdminError::Redis(String::new()), 503, "redis"),
            (AdminError::Upstream(String::new()), 502, "upstream"),
            (AdminError::Rejected(String::new()), 422, "rejected"),
            (AdminError::Internal(String::new()), 500, "internal"),
        ];
        for (error, status, code) in cases {
            let (rendered_status, body) = rendered(error).await;
            assert_eq!(rendered_status.as_u16(), status, "{code}");
            assert_eq!(body["error"]["code"], code);
        }
    }

    #[test]
    fn context_prefixes_the_message_and_keeps_the_kind() {
        let error: Result<(), AdminError> = Err(AdminError::Redis("connection refused".into()));
        assert_eq!(
            error.context("read the session epoch").unwrap_err(),
            AdminError::Redis("read the session epoch: connection refused".into())
        );
    }

    #[test]
    fn exhausted_pool_counts_as_a_timeout() {
        let error = AdminError::from(sqlx::Error::PoolTimedOut);
        assert_eq!(error.code(), "timeout");
    }

    #[tokio::test]
    async fn canceled_statements_map_to_timeout() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL statement_timeout = '50ms'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let error = sqlx::query("SELECT pg_sleep(1)")
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(AdminError::from(error).code(), "timeout");
    }

    #[test]
    fn other_database_errors_stay_database_errors() {
        let error = AdminError::from(sqlx::Error::RowNotFound);
        assert_eq!(error.code(), "database");
    }
}
