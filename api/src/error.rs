use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Fejl der sendes til klienten som `{ "error": "..." }`.
pub struct AppError(pub StatusCode, pub String);

pub type R<T> = Result<T, AppError>;

impl AppError {
    pub fn not_found(what: &str) -> Self {
        AppError(StatusCode::NOT_FOUND, format!("{what} findes ikke"))
    }
    pub fn bad(msg: impl Into<String>) -> Self {
        AppError(StatusCode::BAD_REQUEST, msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError(StatusCode::CONFLICT, msg.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        let e = e.into();
        tracing::error!("{e:#}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}
