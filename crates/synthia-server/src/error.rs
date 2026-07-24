//! Server error and response types
//!
//! Provides comprehensive error handling with HTTP status mapping
//! and unified response format for all API endpoints.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug)]
pub enum ServerError {
    Internal(String),
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    Conflict(String),
    TooManyRequests(String),
    ServiceUnavailable(String),
    AgentError(String),
    McpError(String),
    ToolError(String),
    SessionError(String),
    ConfigError(String),
}

impl ServerError {
    pub fn not_found(entity: &str, id: &str) -> Self {
        ServerError::NotFound(format!("{} '{}' not found", entity, id))
    }

    pub fn already_exists(entity: &str, id: &str) -> Self {
        ServerError::Conflict(format!("{} '{}' already exists", entity, id))
    }

    pub fn invalid_input(field: &str, reason: &str) -> Self {
        ServerError::BadRequest(format!("Invalid {}: {}", field, reason))
    }

    pub fn missing_field(field: &str) -> Self {
        ServerError::BadRequest(format!("Missing required field: {}", field))
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match self {
            ServerError::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg)
            }
            ServerError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, "not_found", msg)
            }
            ServerError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, "bad_request", msg)
            }
            ServerError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, "unauthorized", msg)
            }
            ServerError::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, "forbidden", msg)
            }
            ServerError::Conflict(msg) => {
                (StatusCode::CONFLICT, "conflict", msg)
            }
            ServerError::TooManyRequests(msg) => {
                (StatusCode::TOO_MANY_REQUESTS, "too_many_requests", msg)
            }
            ServerError::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", msg)
            }
            ServerError::AgentError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "agent_error", msg)
            }
            ServerError::McpError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "mcp_error", msg)
            }
            ServerError::ToolError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "tool_error", msg)
            }
            ServerError::SessionError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "session_error", msg)
            }
            ServerError::ConfigError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "config_error", msg)
            }
        };

        let body = Json(json!({
            "error": {
                "type": error_type,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for ServerError {
    fn from(e: anyhow::Error) -> Self {
        ServerError::Internal(e.to_string())
    }
}

impl From<std::io::Error> for ServerError {
    fn from(e: std::io::Error) -> Self {
        ServerError::Internal(e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(error_type: &str, message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiError {
                error_type: error_type.to_string(),
                message,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResponse {
    pub success: bool,
}

impl EmptyResponse {
    pub fn success() -> Self {
        Self { success: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedResponse<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
    pub has_more: bool,
}

impl<T> PagedResponse<T> {
    pub fn new(items: Vec<T>, mark: Option<String>, has_more: bool) -> Self {
        Self {
            items,
            mark,
            has_more,
        }
    }
}
