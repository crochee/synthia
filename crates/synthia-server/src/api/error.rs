//! Structured API error responses for V2 endpoints.
//!
//! All V2 error bodies conform to the OpenSpec envelope:
//! `{ "error": { "code": "...", "message": "...", "details": [...] } }`.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Structured API error returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Vec<ErrorDetail>,
}

/// Per-field or per-constraint error detail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub message: String,
    pub code: String,
}

impl ErrorDetail {
    pub fn new(
        field: Option<impl Into<String>>,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self {
            field: field.map(Into::into),
            message: message.into(),
            code: code.into(),
        }
    }
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Vec::new(),
        }
    }

    pub fn with_details(mut self, details: Vec<ErrorDetail>) -> Self {
        self.details = details;
        self
    }

    pub fn not_found(resource: impl Into<String>) -> Self {
        let resource = resource.into();
        Self::new("not_found", format!("{} not found", resource))
    }

    pub fn unauthorized() -> Self {
        Self::new("unauthorized", "Unauthorized")
    }

    pub fn forbidden() -> Self {
        Self::new("forbidden", "Forbidden")
    }

    pub fn validation_error(details: Vec<ErrorDetail>) -> Self {
        Self {
            code: "validation_error".to_string(),
            message: "Validation failed".to_string(),
            details,
        }
    }

    pub fn invalid_cursor() -> Self {
        Self::new("invalid_cursor", "Invalid or malformed cursor")
    }

    pub fn invalid_state_transition() -> Self {
        Self::new("invalid_state_transition", "Invalid state transition")
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal_error", message.into())
    }

    fn status_code(&self) -> StatusCode {
        match self.code.as_str() {
            "not_found" => StatusCode::NOT_FOUND,
            "unauthorized" => StatusCode::UNAUTHORIZED,
            "forbidden" => StatusCode::FORBIDDEN,
            "validation_error"
            | "invalid_cursor"
            | "invalid_state_transition" => StatusCode::BAD_REQUEST,
            "conflict" => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(json!({ "error": self }));
        (status, body).into_response()
    }
}

impl From<synthia_session::session::SessionError> for ApiError {
    fn from(err: synthia_session::session::SessionError) -> Self {
        match err {
            synthia_session::session::SessionError::NotFound => {
                Self::not_found("session")
            }
            synthia_session::session::SessionError::Unauthorized => {
                Self::forbidden()
            }
            synthia_session::session::SessionError::Session(msg) => {
                Self::internal(msg)
            }
            synthia_session::session::SessionError::Io(e) => {
                Self::internal(e.to_string())
            }
            synthia_session::session::SessionError::Serialization(e) => {
                Self::internal(e.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_has_404_status() {
        let response = ApiError::not_found("session").into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn unauthorized_has_401_status() {
        let response = ApiError::unauthorized().into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn forbidden_has_403_status() {
        let response = ApiError::forbidden().into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn validation_error_has_400_status() {
        let response = ApiError::validation_error(vec![ErrorDetail::new(
            Some("name"),
            "name is required",
            "required",
        )])
        .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_cursor_has_400_status() {
        let response = ApiError::invalid_cursor().into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_state_transition_has_400_status() {
        let response = ApiError::invalid_state_transition().into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn internal_has_500_status() {
        let response =
            ApiError::internal("database unavailable").into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn from_session_error_maps_not_found_to_404() {
        let err: ApiError =
            synthia_session::session::SessionError::NotFound.into();
        assert_eq!(err.code, "not_found");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn from_session_error_maps_unauthorized_to_403() {
        let err: ApiError =
            synthia_session::session::SessionError::Unauthorized.into();
        assert_eq!(err.code, "forbidden");
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn from_session_error_maps_session_to_500() {
        let err: ApiError =
            synthia_session::session::SessionError::Session("boom".to_string())
                .into();
        assert_eq!(err.code, "internal_error");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn error_body_matches_openspec_shape() {
        let response = ApiError::not_found("session").into_response();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "not_found");
        assert!(!value["error"]["message"].as_str().unwrap().is_empty());
        assert!(value["error"]["details"].is_array());
    }
}
