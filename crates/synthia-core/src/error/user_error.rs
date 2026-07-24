//! The [`UserError`] struct — a user-facing (safe-to-display)
//! wrapper carrying an [`ErrorCode`], message, and optional
//! structured details. Implements `Display`, `std::error::Error`,
//! and four `From` impls.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{error::Error, error_code::ErrorCode};

/// A structured, user-facing error suitable for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl UserError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details),
        }
    }
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for UserError {}

impl From<&str> for UserError {
    fn from(s: &str) -> Self {
        UserError::new(ErrorCode::InternalServerError, s)
    }
}

impl From<String> for UserError {
    fn from(s: String) -> Self {
        UserError::new(ErrorCode::InternalServerError, s)
    }
}

impl From<Error> for UserError {
    fn from(err: Error) -> Self {
        UserError::new(err.code(), err.to_string())
    }
}
