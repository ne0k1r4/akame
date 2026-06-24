// error.rs — centralised error types
// thiserror keeps the boilerplate out of my face

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PhantomError {
    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("task queue full for session {0}")]
    QueueFull(String),

    #[error("implant sent malformed beacon: {0}")]
    MalformedBeacon(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    // catch-all for anyhow errors bubbling up into axum handlers
    #[error("internal: {0}")]
    Internal(String),
}

// lets axum return our errors as HTTP responses
impl axum::response::IntoResponse for PhantomError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        use axum::Json;
        use serde_json::json;

        let (status, msg) = match &self {
            PhantomError::SessionNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            PhantomError::QueueFull(_)       => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            PhantomError::MalformedBeacon(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            _                                => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        (status, Json(json!({ "error": msg }))).into_response()
    }
}
// error types
