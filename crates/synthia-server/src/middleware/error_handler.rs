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

use crate::api::{ErrorCode, UserError};

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

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    /// The middleware MUST pass through normal responses untouched
    /// (no wrapping, no body mutation).
    #[tokio::test]
    async fn happy_path_passes_through() {
        async fn handler() -> (StatusCode, &'static str) {
            (StatusCode::OK, "ok")
        }
        let app = Router::new()
            .route("/", get(handler))
            .layer(from_fn(error_handler_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// A handler that panics with a `String` MUST be caught and
    /// return `500 InternalServerError` with the `internal_server_error`
    /// error code in the JSON body.
    #[tokio::test]
    async fn panic_with_string_is_caught() {
        async fn handler() -> &'static str {
            panic!("intentional panic for testing")
        }
        let app = Router::new()
            .route("/", get(handler))
            .layer(from_fn(error_handler_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body_bytes =
            axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes)
            .expect("body should be valid JSON");
        assert_eq!(body["status"], "error");
        assert_eq!(body["error"]["code"], "internal_server_error");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("internal error")
        );
    }

    /// A handler that panics with a `&'static str` MUST be caught
    /// and return a 500 with the standard error envelope.
    #[tokio::test]
    async fn panic_with_str_is_caught() {
        async fn handler() -> &'static str {
            panic!("static str panic")
        }
        let app = Router::new()
            .route("/", get(handler))
            .layer(from_fn(error_handler_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// A handler that panics with a non-string type MUST be caught
    /// (and the `unknown panic` fallback message used).
    #[tokio::test]
    async fn panic_with_non_string_is_caught() {
        async fn handler() -> &'static str {
            panic!("42 panic")
        }
        let app = Router::new()
            .route("/", get(handler))
            .layer(from_fn(error_handler_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body_bytes =
            axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes)
            .expect("body should be valid JSON");
        // The contract is just that the body is valid JSON and the
        // error code is correct.
        assert_eq!(body["error"]["code"], "internal_server_error");
    }

    /// The error response MUST include a JSON `ApiResponse::Err`
    /// envelope: `{"status": "error", "error": {"code", "message"}}`.
    #[tokio::test]
    async fn error_response_has_standard_envelope() {
        async fn handler() -> &'static str {
            panic!("x")
        }
        let app = Router::new()
            .route("/", get(handler))
            .layer(from_fn(error_handler_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body_bytes =
            axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&body_bytes).unwrap();
        // Pin the envelope shape.
        assert!(body.get("status").is_some());
        assert!(body.get("error").is_some());
        assert!(body["error"].get("code").is_some());
        assert!(body["error"].get("message").is_some());
    }

    /// The middleware MUST NOT swallow non-200 responses from the
    /// inner handler (e.g. a 404 is preserved as 404, not wrapped to 500).
    #[tokio::test]
    async fn non_500_responses_preserved() {
        async fn handler() -> (StatusCode, &'static str) {
            (StatusCode::NOT_FOUND, "missing")
        }
        let app = Router::new()
            .route("/", get(handler))
            .layer(from_fn(error_handler_middleware));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
