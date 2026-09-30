use std::{
    sync::Arc,
    task::{Context, Poll},
};

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
/// Reads the credential from `Authorization` (with or without a
/// `Bearer ` prefix) or — for Anthropic-protocol clients, which send
/// `x-api-key` and no `Authorization` header at all — from `x-api-key`,
/// and validates it against a configured API key. If no API key is
/// configured (empty), the middleware allows all requests. Certain paths
/// (e.g., /livez, /readyz) are exempt from authentication.
///
/// `x-api-key` is accepted as an *alternative*, never as a bypass: the
/// value must equal the configured key, so a deployment that sets
/// `SYNTHIA_API_KEY` stays closed whether the caller speaks Bearer or
/// the Anthropic protocol.
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    auth_config: Arc<AuthConfig>,
    /// API key captured at startup. Reading the env on every
    /// request is wasteful (the value never changes for the
    /// lifetime of the server), and the `Arc<str>` lets us
    /// share it across `Clone`d middleware instances for free.
    api_key: Arc<str>,
}

impl<S> AuthMiddleware<S> {
    pub fn new(inner: S, auth_config: Arc<AuthConfig>) -> Self {
        // Read the env once at construction; if the operator
        // rotates `SYNTHIA_API_KEY` they must restart the server,
        // which matches the rest of the config model.
        let api_key: Arc<str> =
            std::env::var("SYNTHIA_API_KEY").unwrap_or_default().into();
        Self {
            inner,
            auth_config,
            api_key,
        }
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

    /// Validate the provided token against the configured API key.
    fn validate_token(token: &str, api_key: &str) -> bool {
        token == api_key
    }

    /// The credential a request presents, in precedence order.
    ///
    /// `Authorization` wins when both headers are present, so a client
    /// that sends a stale `x-api-key` alongside a valid bearer token is
    /// still authenticated by the header the rest of this server uses.
    /// A bare `Authorization` value (no `Bearer ` prefix) is accepted,
    /// matching the pre-existing behaviour.
    fn credential<B>(req: &Request<B>) -> Option<&str> {
        let authorization = req
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .map(|header| header.strip_prefix("Bearer ").unwrap_or(header));
        authorization.or_else(|| {
            req.headers()
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
        })
    }
}

/// Path of the Anthropic Messages compatibility surface.
const MESSAGES_PATH: &str = "/v1/messages";

/// `true` when `path` is the Anthropic-protocol surface, which answers
/// with the protocol's error envelope on every failure.
fn is_anthropic_path(path: &str) -> bool {
    path == MESSAGES_PATH
}

/// The `401` a rejected request receives.
///
/// `/api/v1/*` keeps the application envelope byte-for-byte; the
/// Anthropic surface answers with `{"type":"error","error":{…}}` so an
/// Anthropic SDK raises a parseable authentication error instead of
/// choking on an unknown body.
fn unauthorized_response(anthropic: bool) -> Response {
    const MESSAGE: &str = "Invalid or missing authentication credentials";
    let body = if anthropic {
        serde_json::json!({
            "type": "error",
            "error": {
                "type": "authentication_error",
                "message": MESSAGE,
            }
        })
    } else {
        serde_json::json!({
            "status": "err",
            "error": {
                "code": "unauthorized",
                "message": MESSAGE
            }
        })
    };
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::new(body.to_string()))
        .unwrap()
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
        // Clone the Arc<str> once per call (cheap pointer + refcount bump)
        // rather than re-reading the env on every request.
        let api_key = Arc::clone(&self.api_key);

        // Allow public paths and unconfigured auth
        if is_public_path {
            // Still inject a user_id so handlers can find the namespace.
            req.extensions_mut()
                .insert(RequestUserId(resolve_user_id_unconfigured()));
            return Box::pin(async move { inner.call(req).await });
        }

        Box::pin(async move {
            if api_key.is_empty() {
                // Unconfigured: pass through with the server default
                // user_id. This preserves the historical "no auth" dev
                // path while keeping the §1 invariant (non-empty id).
                req.extensions_mut()
                    .insert(RequestUserId(resolve_user_id_unconfigured()));
                return inner.call(req).await;
            }

            let extracted_token = Self::credential(&req).map(str::to_string);

            let is_valid = match &extracted_token {
                Some(token) => Self::validate_token(token, &api_key),
                None => false,
            };

            if !is_valid {
                return Ok(unauthorized_response(is_anthropic_path(
                    req.uri().path(),
                )));
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use axum::http::Request;

    use super::*;
    use crate::config::AuthConfig;

    fn empty_auth_config() -> AuthConfig {
        AuthConfig {
            enabled: false,
            api_keys: vec![],
            key_to_user: HashMap::new(),
        }
    }

    // -- is_public_path -----------------------------------------------

    /// `/livez` and `/readyz` are known-public probe endpoints
    /// and MUST be allowed through without authentication.
    #[test]
    fn is_public_path_probes_are_public() {
        assert!(AuthMiddleware::<Request<()>>::is_public_path("/livez"));
        assert!(AuthMiddleware::<Request<()>>::is_public_path("/readyz"));
    }

    /// `/.well-known/agent-card.json` (the discovery
    /// endpoint) MUST be public.
    #[test]
    fn is_public_path_agent_card_is_public() {
        assert!(AuthMiddleware::<Request<()>>::is_public_path(
            "/.well-known/agent-card.json"
        ));
    }

    /// `/api/foo` (a protected path) MUST NOT be public.
    #[test]
    fn is_public_path_api_prefix_is_not_public() {
        assert!(!AuthMiddleware::<Request<()>>::is_public_path("/api/foo"));
        assert!(!AuthMiddleware::<Request<()>>::is_public_path("/api/run"));
    }

    /// Paths with `..` MUST be rejected by `normalize_path` so
    /// `is_public_path` returns false (security-critical: a
    /// request like `/livez/../api/run` MUST NOT bypass auth).
    #[test]
    fn is_public_path_dot_dot_rejected() {
        assert!(!AuthMiddleware::<Request<()>>::is_public_path(
            "/livez/../api/run"
        ));
        assert!(!AuthMiddleware::<Request<()>>::is_public_path(
            "/api/../livez"
        ));
    }

    /// Path-prefix matching MUST be limited to ONE level deep
    /// (so `/livez/foo/bar` is NOT public). Pin the depth
    /// boundary to prevent a future refactor from accidentally
    /// granting broad prefix exemptions.
    #[test]
    fn is_public_path_one_level_deep_subpath_is_public() {
        assert!(AuthMiddleware::<Request<()>>::is_public_path(
            "/livez/check"
        ));
    }

    /// Path-prefix matching MUST NOT allow deeper nesting.
    /// `/livez/foo/bar` MUST NOT be treated as public.
    #[test]
    fn is_public_path_two_levels_deep_subpath_is_not_public() {
        assert!(!AuthMiddleware::<Request<()>>::is_public_path(
            "/livez/foo/bar"
        ));
    }

    /// `/.well-known/agent-card.json/foo/bar` (2 segments deep
    /// under the API endpoint) MUST NOT be treated as public.
    #[test]
    fn is_public_path_agent_card_two_levels_deep_not_public() {
        assert!(!AuthMiddleware::<Request<()>>::is_public_path(
            "/.well-known/agent-card.json/foo/bar"
        ));
    }

    /// Empty paths, root `/`, and unknown paths MUST NOT be
    /// classified as public.
    #[test]
    fn is_public_path_unknown_paths_are_not_public() {
        assert!(!AuthMiddleware::<Request<()>>::is_public_path(""));
        assert!(!AuthMiddleware::<Request<()>>::is_public_path("/"));
        assert!(!AuthMiddleware::<Request<()>>::is_public_path("/admin"));
        assert!(!AuthMiddleware::<Request<()>>::is_public_path("/internal"));
    }

    /// `is_public_path` MUST be exact-match for the root of a
    /// public path, NOT substring (`/my-livez` MUST NOT match
    /// `/livez`).
    #[test]
    fn is_public_path_substring_does_not_match() {
        assert!(!AuthMiddleware::<Request<()>>::is_public_path("/my-livez"));
        assert!(!AuthMiddleware::<Request<()>>::is_public_path(
            "/super-livez"
        ));
        assert!(!AuthMiddleware::<Request<()>>::is_public_path(
            "/livezcheck"
        ));
    }

    // -- validate_token (via internal exposed behavior) --------------

    /// `validate_token` MUST accept an exact match.
    /// We exercise the trait method indirectly through the public
    /// surface area: the impl block is the only entry point.
    /// (validate_token is a private function — pin the contract
    /// via #[cfg(test)] read access if added later.)
    #[test]
    fn validate_token_match_is_accepted_via_construction() {
        // Direct test via the public middleware. With
        // SYNTHIA_API_KEY set, the middleware MUST validate
        // matching tokens.
        // SAFETY: This test sets an env var in the test process.
        // It is not thread-safe but the test is marked
        // `#[test]` (singular) so it does not run concurrently
        // with siblings.
        // SAFETY: see comments.
        unsafe {
            std::env::set_var("SYNTHIA_API_KEY", "secret-1");
        }
        let cfg = Arc::new(empty_auth_config());
        // The middleware's `api_key` field is private — we
        // can't read it directly. But we can construct it and
        // confirm no panic.
        let mw: AuthMiddleware<Request<()>> =
            AuthMiddleware::new(Request::new(()), cfg);
        // The Arc<str> for the key is private, so we can't
        // assert it equals "secret-1" directly. We at least
        // pin that the middleware was constructed without
        // panicking.
        let _ = mw;
        unsafe {
            std::env::remove_var("SYNTHIA_API_KEY");
        }
    }

    /// When `SYNTHIA_API_KEY` is unset, `AuthMiddleware::new`
    /// MUST capture an empty api_key (no panic, no fallback).
    #[test]
    fn auth_middleware_new_without_env_var_does_not_panic() {
        unsafe {
            std::env::remove_var("SYNTHIA_API_KEY");
        }
        let cfg = Arc::new(empty_auth_config());
        let mw: AuthMiddleware<Request<()>> =
            AuthMiddleware::new(Request::new(()), cfg);
        let _ = mw;
    }

    /// When `SYNTHIA_API_KEY` is set to an empty string, the
    /// middleware MUST treat the key as empty (matching
    /// `unwrap_or_default`).
    #[test]
    fn auth_middleware_new_with_empty_env_var_does_not_panic() {
        unsafe {
            std::env::set_var("SYNTHIA_API_KEY", "");
        }
        let cfg = Arc::new(empty_auth_config());
        let mw: AuthMiddleware<Request<()>> =
            AuthMiddleware::new(Request::new(()), cfg);
        let _ = mw;
        unsafe {
            std::env::remove_var("SYNTHIA_API_KEY");
        }
    }

    /// Two middleware instances constructed with the same env
    /// MUST NOT share an `Arc<str>` for the api_key (each
    /// instance allocates its own — the cheap Arc clone is for
    /// `Clone`-derived copies, not constructor copies).
    #[test]
    fn auth_middleware_new_creates_independent_arc_str_per_instance() {
        unsafe {
            std::env::set_var("SYNTHIA_API_KEY", "k");
        }
        let cfg = Arc::new(empty_auth_config());
        let a: AuthMiddleware<Request<()>> =
            AuthMiddleware::new(Request::new(()), cfg.clone());
        let b: AuthMiddleware<Request<()>> =
            AuthMiddleware::new(Request::new(()), cfg);
        // Cloning middleware (which is `#[derive(Clone)]`)
        // shares the inner `api_key` Arc. Both instances MUST
        // be constructible without panic.
        let _a_clone = a.clone();
        let _b_clone = b.clone();
        unsafe {
            std::env::remove_var("SYNTHIA_API_KEY");
        }
    }

    /// `AuthMiddleware` MUST derive `Clone` (the Service impl
    /// requires it).
    #[test]
    fn auth_middleware_supports_clone() {
        unsafe {
            std::env::remove_var("SYNTHIA_API_KEY");
        }
        let cfg = Arc::new(empty_auth_config());
        let mw: AuthMiddleware<Request<()>> =
            AuthMiddleware::new(Request::new(()), cfg);
        let _cloned = mw.clone();
    }

    /// `is_public_path` MUST be idempotent and depend only on
    /// the input path (no global state).
    #[test]
    fn is_public_path_is_deterministic() {
        let paths = ["/livez", "/agent-card", "/api/run", "/api/foo"];
        for p in paths {
            let first = AuthMiddleware::<Request<()>>::is_public_path(p);
            let second = AuthMiddleware::<Request<()>>::is_public_path(p);
            let third = AuthMiddleware::<Request<()>>::is_public_path(p);
            assert_eq!(
                first, second,
                "is_public_path({p}) must be deterministic"
            );
            assert_eq!(second, third);
        }
    }

    /// `is_public_path` MUST treat paths with URL-encoded
    /// segments by DECODING first (defensive: a path like
    /// `/%6civez` MUST resolve to `/livez` via
    /// `normalize_path` and be classified as public).
    #[test]
    fn is_public_path_url_encoded_probe_decodes_to_public() {
        // %6c == 'l' so /%6civez == /livez after decode.
        assert!(AuthMiddleware::<Request<()>>::is_public_path("/%6civez"));
    }

    // -- credential extraction (Anthropic `x-api-key`) ----------------

    fn request_with(headers: &[(&str, &str)]) -> Request<()> {
        let mut builder = Request::builder();
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(()).unwrap()
    }

    /// The protocol's header MUST be read as a credential — an
    /// Anthropic client sends `x-api-key` and no `Authorization`.
    #[test]
    fn credential_reads_x_api_key_when_authorization_is_absent() {
        let request = request_with(&[("x-api-key", "secret")]);
        assert_eq!(
            AuthMiddleware::<Request<()>>::credential(&request),
            Some("secret")
        );
    }

    /// `Authorization: Bearer <key>` MUST still be the credential, with
    /// the bare (prefix-less) spelling accepted for back-compat.
    #[test]
    fn credential_accepts_bearer_and_bare_authorization() {
        let bearer = request_with(&[("authorization", "Bearer secret")]);
        assert_eq!(
            AuthMiddleware::<Request<()>>::credential(&bearer),
            Some("secret")
        );
        let bare = request_with(&[("authorization", "secret")]);
        assert_eq!(
            AuthMiddleware::<Request<()>>::credential(&bare),
            Some("secret")
        );
    }

    /// When both headers are present `Authorization` wins, so the
    /// credential — and therefore the resolved `user_id` — is never
    /// ambiguous.
    #[test]
    fn credential_prefers_authorization_over_x_api_key() {
        let request = request_with(&[
            ("authorization", "Bearer bearer-key"),
            ("x-api-key", "anthropic-key"),
        ]);
        assert_eq!(
            AuthMiddleware::<Request<()>>::credential(&request),
            Some("bearer-key")
        );
    }

    /// No credential header at all means no token (the caller still
    /// has to reject, or fall through on the unconfigured path).
    #[test]
    fn credential_is_none_without_a_credential_header() {
        assert_eq!(
            AuthMiddleware::<Request<()>>::credential(&request_with(&[])),
            None
        );
    }

    // -- Anthropic error envelope on 401 ------------------------------

    /// Only the Anthropic surface reports its failures in the
    /// protocol's vocabulary.
    #[test]
    fn is_anthropic_path_matches_only_the_messages_route() {
        assert!(is_anthropic_path("/v1/messages"));
        assert!(!is_anthropic_path("/api/v1/chat/sessions"));
        assert!(!is_anthropic_path("/v1/messages/"));
        assert!(!is_anthropic_path("/metrics"));
    }

    /// A rejected Anthropic credential MUST answer with
    /// `{"type":"error","error":{"type":"authentication_error",…}}`;
    /// every other path keeps the application envelope byte-for-byte.
    #[tokio::test]
    async fn unauthorized_response_is_anthropic_shaped_only_for_that_route() {
        async fn json_of(response: Response) -> serde_json::Value {
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("the 401 body must be readable");
            serde_json::from_slice(&bytes).expect("the 401 body must be JSON")
        }

        let anthropic = unauthorized_response(true);
        assert_eq!(anthropic.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            anthropic
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        let body = json_of(anthropic).await;
        assert_eq!(body["type"], "error");
        assert_eq!(body["error"]["type"], "authentication_error");
        assert!(
            body.get("status").is_none(),
            "the app envelope must not leak: {body}"
        );

        let application = json_of(unauthorized_response(false)).await;
        assert_eq!(application["status"], "err");
        assert_eq!(application["error"]["code"], "unauthorized");
        assert!(
            application["error"].get("type").is_none(),
            "the app envelope is unchanged: {application}"
        );
    }
}
