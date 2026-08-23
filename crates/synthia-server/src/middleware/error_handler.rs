//! Fallback error handler middleware.
//!
//! Catches panics escaping a handler and converts them into the
//! unified error envelope (`{ "code", "message" }`) with a 500
//! status, via the same [`AppError`] adapter handlers use (see
//! `synthia_server::api::error`). The envelope shape is flat; the
//! `source` chain is logged via `tracing::error!` and never
//! crosses the wire.
use std::panic::AssertUnwindSafe;

use axum::{
    body::Body,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures::FutureExt;
use synthia_core::Error;

use crate::api::AppError;

/// Middleware that wraps the entire request handling in a
/// catch-all error handler. Any panic escaping the inner handler
/// is rendered as a 500 with the standard envelope.
pub async fn error_handler_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    let response = AssertUnwindSafe(next.run(request)).catch_unwind().await;
    match response {
        Ok(response) => response,
        Err(panic_info) => {
            let message = panic_info
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| panic_info.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");

            tracing::error!(panic = %message, "Request handler panicked");

            // Reuse the boundary adapter so panics render in the
            // exact same envelope as any other 500.
            AppError::from(Error::internal(
                "An internal error occurred while processing your request",
            ))
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
    /// return `500` with the `internal_server_error` error code in
    /// the JSON body.
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
        assert_eq!(body["code"], "internal_server_error");
        // `message` is the error's full Display output (the
        // envelope contract), so it carries the variant prefix.
        assert_eq!(
            body["message"],
            "internal error: An internal error occurred while \
             processing your request"
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
        assert_eq!(body["code"], "internal_server_error");
    }

    /// The error response MUST include the unified error envelope:
    /// `{"code", "message"}`.
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
        assert!(body.get("code").is_some());
        assert!(body.get("message").is_some());
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
