//! Fallback error handler middleware.
//!
//! Catches panics and unhandled errors, returning a consistent ApiResponse::Err
//! with InternalServerError status code.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::IntoResponse,
};
use futures::FutureExt;
use synthia_core::{ErrorCode, UserError};

/// Middleware that wraps the entire request handling in a catch-all error handler.
///
/// This ensures that any panic or unhandled error results in a proper JSON
/// ApiResponse::Err response instead of a raw HTTP 500.
pub async fn error_handler_middleware(
    request: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let future = next.run(request);

    let result = std::panic::AssertUnwindSafe(future).catch_unwind().await;

    match result {
        Ok(response) => response,
        Err(panic_info) => {
            let message = panic_info
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| panic_info.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");

            tracing::error!(panic = %message, "Request handler panicked");

            let error = UserError::new(
                ErrorCode::InternalServerError,
                "An internal error occurred while processing your request",
            );
            let body = serde_json::json!({
                "status": "error",
                "error": {
                    "code": error.code.to_string(),
                    "message": error.message,
                }
            });

            (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(body))
                .into_response()
        }
    }
}
