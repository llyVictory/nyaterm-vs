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
    pub fn io(error: &std::io::Error, message: &'static str) -> Self {
        tracing::warn!(event="io.error", error_kind="io", error_code=error.raw_os_error(), reason=?error.kind(), error_hash=%crate::observability::fingerprint(&error.to_string()), "I/O operation failed");
        Self::bad(message)
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
    fn from(error: nyaterm_core::error::AppError) -> Self {
        crate::observability::record_error(&error);
        Self::bad("Backend operation failed")
    }
}
impl From<serde_json::Error> for WebError {
    fn from(_: serde_json::Error) -> Self {
        Self::bad("Invalid request parameters")
    }
}
impl From<russh::Error> for WebError {
    fn from(error: russh::Error) -> Self {
        tracing::warn!(event="ssh.error", error_kind="ssh", reason=crate::observability::error_reason(&error.to_string()), error_hash=%crate::observability::fingerprint(&error.to_string()), "SSH operation failed");
        Self::bad("SSH operation failed")
    }
}
impl From<russh_sftp::client::error::Error> for WebError {
    fn from(error: russh_sftp::client::error::Error) -> Self {
        crate::observability::record_error(&nyaterm_core::error::AppError::Sftp(error));
        Self::bad("SFTP operation failed")
    }
}
impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        tracing::warn!(event="api.error_response", status=self.0.as_u16(), reason=crate::observability::error_reason(&self.1), error_hash=%crate::observability::fingerprint(&self.1), "API operation rejected");
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
