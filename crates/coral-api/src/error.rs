use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Forbidden(String),
    Conflict(String),
    RateLimited,
    ExternalApi(String),
    Internal(String),
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ErrorResponse {
    pub error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "Rate limited".into()),
            ApiError::ExternalApi(msg) => (StatusCode::BAD_GATEWAY, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

impl From<clients::ClientError> for ApiError {
    fn from(err: clients::ClientError) -> Self {
        match err {
            clients::ClientError::PlayerNotFound(p) => {
                ApiError::NotFound(format!("Player not found: {p}"))
            }
            clients::ClientError::RateLimited => ApiError::RateLimited,
            clients::ClientError::HypixelApi(msg) => ApiError::ExternalApi(msg),
            clients::ClientError::InvalidUuid(u) => {
                ApiError::BadRequest(format!("Invalid UUID: {u}"))
            }
            _ => ApiError::Internal(err.to_string()),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Internal(format!("Database error: {err}"))
    }
}
