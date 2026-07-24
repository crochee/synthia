//! The [`ApiResponse<T>`] enum — a JSON-RPC-style envelope with
//! `Ok { data }` and `Err { error: UserError }` variants.
//!
//! The `tag = "status"` serde attribute means the wire format is:
//! - `Ok`: `{ "status": "ok", "data": T }`
//! - `Err`: `{ "status": "err", "error": UserError }`

use serde::Serialize;

use super::{error::Error, user_error::UserError};

/// JSON-RPC-style response envelope.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ApiResponse<T: Serialize> {
    Ok { data: T },
    Err { error: UserError },
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        ApiResponse::Ok { data }
    }

    pub fn err(error: UserError) -> Self {
        ApiResponse::Err { error }
    }

    pub fn from_error(err: Error) -> Self {
        ApiResponse::Err {
            error: UserError::from(err),
        }
    }
}

impl<T: Serialize> From<Result<T, UserError>> for ApiResponse<T> {
    fn from(result: Result<T, UserError>) -> Self {
        match result {
            Ok(data) => ApiResponse::ok(data),
            Err(error) => ApiResponse::err(error),
        }
    }
}

impl<T: Serialize> From<Result<T, Error>> for ApiResponse<T> {
    fn from(result: Result<T, Error>) -> Self {
        match result {
            Ok(data) => ApiResponse::ok(data),
            Err(error) => ApiResponse::from_error(error),
        }
    }
}
