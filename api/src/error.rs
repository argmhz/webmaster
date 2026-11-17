use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

/// Fejl der sendes til klienten som `{ "error": "...", "issues": [...] }`.
/// `issues` er kun med, når en kladde ikke kan publiceres.
pub struct AppError(pub StatusCode, pub String, pub Option<Value>);

pub type R<T> = Result<T, AppError>;

impl AppError {
    pub fn not_found(what: &str) -> Self {
        AppError(StatusCode::NOT_FOUND, format!("{what} findes ikke"), None)
    }
    pub fn bad(msg: impl Into<String>) -> Self {
        AppError(StatusCode::BAD_REQUEST, msg.into(), None)
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError(StatusCode::CONFLICT, msg.into(), None)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.1 });
        if let Some(issues) = self.2 {
            body["issues"] = issues;
        }
        (self.0, Json(body)).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        let e = e.into();
        tracing::error!("{e:#}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"), None)
    }
}
