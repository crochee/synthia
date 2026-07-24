use std::task::{Context, Poll};

use axum::{
    http::{Request, StatusCode},
    response::Response,
};
use futures::future::BoxFuture;
use tower::Service;

use super::{
    path::{PUBLIC_PATHS, normalize_path},
    types::RequestUserId,
    user_id::{resolve_user_id_from_key, resolve_user_id_unconfigured},
};
use crate::config::AuthConfig;

/// Authentication middleware that validates API Key or Bearer Token.
///
/// Reads the `Authorization` header and validates against a configured API key.
/// If no API key is configured (empty), the middleware allows all requests.
/// Certain paths (e.g., /health) are exempt from authentication.
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    auth_config: std::sync::Arc<AuthConfig>,
}

impl<S> AuthMiddleware<S> {
    pub fn new(inner: S, auth_config: std::sync::Arc<AuthConfig>) -> Self {
        Self { inner, auth_config }
    }

    /// Check if a request path should bypass authentication.
    pub(super) fn is_public_path(path: &str) -> bool {
        let normalized = match normalize_path(path) {
            Some(p) => p,
            None => return false,
        };
        PUBLIC_PATHS.iter().any(|public| {
            if normalized == *public {
                return true;
            }
            if normalized.starts_with(&format!("{public}/")) {
                let remaining = &normalized[public.len() + 1..];
                return !remaining.contains('/');
            }
            false
        })
    }

    /// Get the current API key from environment (read at request time).
    fn get_api_key() -> String {
        std::env::var("SYNTHIA_API_KEY").unwrap_or_default()
    }

    /// Validate the provided token against the configured API key.
    fn validate_token(token: &str, api_key: &str) -> bool {
        token == api_key
    }
}

impl<S, B> Service<Request<B>> for AuthMiddleware<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = S::Response;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        let is_public_path = Self::is_public_path(req.uri().path());
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let auth_config = self.auth_config.clone();

        // Allow public paths and unconfigured auth
        if is_public_path {
            // Still inject a user_id so handlers can find the namespace.
            req.extensions_mut()
                .insert(RequestUserId(resolve_user_id_unconfigured()));
            return Box::pin(async move { inner.call(req).await });
        }

        Box::pin(async move {
            let api_key = Self::get_api_key();
            if api_key.is_empty() {
                // Unconfigured: pass through with the server default
                // user_id. This preserves the historical "no auth" dev
                // path while keeping the §1 invariant (non-empty id).
                req.extensions_mut()
                    .insert(RequestUserId(resolve_user_id_unconfigured()));
                return inner.call(req).await;
            }

            let auth_header = req
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok());

            let extracted_token: Option<String> = match auth_header {
                Some(header) => header
                    .strip_prefix("Bearer ")
                    .map(|s| s.to_string())
                    .or_else(|| Some(header.to_string())),
                None => None,
            };

            let is_valid = match &extracted_token {
                Some(token) => Self::validate_token(token, &api_key),
                None => false,
            };

            if !is_valid {
                let response = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::new(
                        serde_json::json!({
                            "status": "err",
                            "error": {
                                "code": "unauthorized",
                                "message": "Invalid or missing authentication credentials"
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap();
                return Ok(response);
            }

            // Auth passed: resolve the user_id from the (validated) key.
            // The key in the header matches api_key, so it is in the
            // auth set; we still call resolve_user_id_from_key so that
            // the explicit map can override the default derivation.
            let user_id = match extracted_token.as_deref() {
                Some(token) => resolve_user_id_from_key(token, &auth_config)
                    .unwrap_or_else(resolve_user_id_unconfigured),
                None => resolve_user_id_unconfigured(),
            };
            req.extensions_mut().insert(RequestUserId(user_id));

            inner.call(req).await
        })
    }
}
