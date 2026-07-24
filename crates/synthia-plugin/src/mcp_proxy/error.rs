//! [`McpProxyError`] — the 8-variant error enum used by
//! every method on [`super::core::McpProxy`].

use thiserror::Error;

use crate::types::McpConfigError;

#[derive(Debug, Error)]
pub enum McpProxyError {
    #[error("server `{0}` already running")]
    ServerAlreadyRunning(String),

    #[error("server `{0}` not found")]
    ServerNotFound(String),

    #[error("server `{0}` failed to start: {1}")]
    StartFailed(String, #[source] std::io::Error),

    #[error("server `{0}` failed to stop: {1}")]
    StopFailed(String, #[source] std::io::Error),

    #[error("server `{0}` validation failed: {1}")]
    ValidationFailed(String, McpConfigError),

    #[error("HTTP request failed: {0}")]
    HttpError(#[source] reqwest::Error),

    #[error("WebSocket connection failed: {0}")]
    WebSocketError(String),

    #[error("connection timeout for server `{0}`")]
    ConnectionTimeout(String),
}
