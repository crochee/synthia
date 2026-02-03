//! Authentication middleware
//!
//! Provides API Key authentication for protected endpoints.

use axum::{
    extract::{Request, State},
    http::{Method, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};

use crate::{AppState, error::ServerError};

pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ServerError> {
    let config = state.config.read().await;

    // Public endpoints: health check and options requests
    let method = request.method();
    let path = request.uri().path();
    if method == Method::OPTIONS || path == "/health" {
        return Ok(next.run(request).await);
    }

    if !config.auth.enabled || config.auth.api_keys.is_empty() {
        return Ok(next.run(request).await);
    }

    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if config.auth.api_keys.iter().any(|k| k == token) {
                Ok(next.run(request).await)
            } else {
                Err(ServerError::Unauthorized("Invalid API key".to_string()))
            }
        }
        _ => Err(ServerError::Unauthorized(
            "Missing or invalid Authorization header".to_string(),
        )),
    }
}
