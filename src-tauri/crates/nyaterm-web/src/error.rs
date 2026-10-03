use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub struct WebError(pub StatusCode, pub String);
pub type Result<T> = std::result::Result<T, WebError>;
impl WebError {
    pub fn bad(message: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }
    pub fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "Access denied".into())
    }
    pub fn unsupported() -> Self {
        Self(
            StatusCode::NOT_IMPLEMENTED,
            "This capability is unavailable in Web mode".into(),
        )
    }
}
impl From<nyaterm_core::error::AppError> for WebError {
    fn from(_: nyaterm_core::error::AppError) -> Self {
        Self::bad("Backend operation failed")
    }
}
impl From<serde_json::Error> for WebError {
    fn from(_: serde_json::Error) -> Self {
        Self::bad("Invalid request parameters")
    }
}
impl From<russh::Error> for WebError {
    fn from(_: russh::Error) -> Self {
        Self::bad("SSH operation failed")
    }
}
impl From<russh_sftp::client::error::Error> for WebError {
    fn from(_: russh_sftp::client::error::Error) -> Self {
        Self::bad("SFTP operation failed")
    }
}
impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
