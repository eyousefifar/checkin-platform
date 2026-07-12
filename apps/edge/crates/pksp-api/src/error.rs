use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub enum AppError {
    Unauthorized(String),
    NotFound(String),
    Conflict(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, detail) = match self {
            Self::Unauthorized(d) => (StatusCode::UNAUTHORIZED, d),
            Self::NotFound(d) => (StatusCode::NOT_FOUND, d),
            Self::Conflict(d) => (StatusCode::CONFLICT, d),
            Self::BadRequest(d) => (StatusCode::BAD_REQUEST, d),
            Self::Internal(d) => (StatusCode::INTERNAL_SERVER_ERROR, d),
        };
        (status, Json(json!({ "detail": detail }))).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}
