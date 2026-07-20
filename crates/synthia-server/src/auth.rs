//! API authentication middleware.
//!
//! Checks for a Bearer token in the Authorization header against a configured API key.
//! Health check and A2A discovery paths bypass authentication.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::IntoResponse,
};
use synthia_core::{ErrorCode, UserError};

/// Paths that bypass authentication.
const PUBLIC_PATHS: &[&str] = &["/health", "/.well-known/agent-card.json"];

/// Extract the configured API key from environment.
fn get_api_key() -> String {
    std::env::var("SYNTHIA_API_KEY").unwrap_or_default()
}

/// Check if a request path should bypass authentication.
fn is_public_path(path: &str) -> bool {
    PUBLIC_PATHS.iter().any(|public| {
        path == *public || path.starts_with(&format!("{}/", public))
    })
}

/// Auth middleware for API routes.
///
/// Validates `Bearer <token>` against the configured SYNTHIA_API_KEY.
/// If no key is configured, all requests are allowed.
pub async fn auth_middleware(
    request: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let path = request.uri().path();

    // Allow public paths (health check, A2A discovery)
    if is_public_path(path) {
        return next.run(request).await;
    }

    let api_key = get_api_key();

    // If no API key is configured, allow all requests
    if api_key.is_empty() {
        return next.run(request).await;
    }

    // Extract and validate Bearer token
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let is_valid = auth_header
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|token| token == api_key);

    if !is_valid {
        let error = UserError::new(
            ErrorCode::Unauthorized,
            "Invalid or missing authentication credentials",
        );
        let body = serde_json::json!({
            "status": "err",
            "error": {
                "code": error.code.to_string(),
                "message": error.message,
            }
        });
        return (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_path_matching() {
        assert!(is_public_path("/health"));
        assert!(is_public_path("/health/check"));
        assert!(is_public_path("/.well-known/agent-card.json"));
        assert!(!is_public_path("/api/providers"));
        assert!(!is_public_path("/api/sessions"));
    }
}
