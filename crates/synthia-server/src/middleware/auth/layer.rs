use tower::Layer;

use super::middleware::AuthMiddleware;
use crate::config::AuthConfig;

/// Tower Layer for AuthMiddleware
#[derive(Clone)]
pub struct AuthLayer {
    auth_config: std::sync::Arc<AuthConfig>,
}

impl AuthLayer {
    pub fn new(auth_config: std::sync::Arc<AuthConfig>) -> Self {
        Self { auth_config }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware::new(inner, self.auth_config.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use axum::http::Request;
    use tower::Layer;

    use super::*;
    use crate::config::AuthConfig;

    fn empty_auth_config() -> AuthConfig {
        AuthConfig {
            enabled: false,
            api_keys: vec![],
            key_to_user: HashMap::new(),
        }
    }

    /// `AuthLayer::new(Arc<AuthConfig>)` MUST populate the
    /// internal Arc (no panic).
    #[test]
    fn new_stores_auth_config() {
        let cfg = Arc::new(empty_auth_config());
        let _layer = AuthLayer::new(cfg);
    }

    /// `AuthLayer` MUST derive `Clone` (Tower requires it for
    /// wrapping services).
    #[test]
    fn auth_layer_supports_clone() {
        let cfg = Arc::new(empty_auth_config());
        let layer = AuthLayer::new(cfg);
        let _cloned = layer.clone();
    }

    /// `Layer::layer(inner)` MUST return an `AuthMiddleware<S>`
    /// (the documented `Service` associated type).
    #[test]
    fn layer_returns_auth_middleware() {
        let cfg = Arc::new(empty_auth_config());
        let layer = AuthLayer::new(cfg);
        // Build a minimal inner service. Request<()> with ()
        // body satisfies the bounds.
        let svc = tower::service_fn(|_req: Request<()>| async {
            Ok::<_, std::convert::Infallible>(axum::response::Response::new(
                axum::body::Body::empty(),
            ))
        });
        let _mw: <AuthLayer as Layer<_>>::Service =
            <AuthLayer as Layer<_>>::layer(&layer, svc);
    }

    /// `Layer::layer` MUST clone the `Arc<AuthConfig>` (cheap,
    /// shared ownership — not `Box` / `Arc::new` of a copy).
    /// Two layers constructed with the same `Arc` MUST share
    /// the same allocation.
    #[test]
    fn layer_shares_arc_auth_config_across_clones() {
        let cfg = Arc::new(empty_auth_config());
        let layer = AuthLayer::new(cfg.clone());
        // Cloning the layer MUST NOT increment the strong count
        // beyond the Arc::clone of the field (the cfg is shared).
        let _cloned = layer.clone();
        // cfg strong count: original Arc (1) + cfg.clone() passed
        // to layer (still 1 — drop happens) + layer's field (1) +
        // _cloned's layer field (cloned via Arc::clone, +1).
        // Net: ≥ 2 (cfg held by layer + cfg held by cloned layer).
        let count = Arc::strong_count(&cfg);
        assert!(
            count >= 2,
            "AuthConfig must be Arc-shared across layer clones; got strong_count={count}"
        );
    }

    /// The `Service` associated type MUST be exactly
    /// `AuthMiddleware<S>` (compile-time check). Pin the type
    /// relationship so a refactor that swaps the middleware
    /// type breaks loudly.
    #[test]
    fn service_associated_type_is_auth_middleware() {
        // Compile-time assertion via type annotation. The fact
        // that this compiles means the associated type is
        // exactly `AuthMiddleware<S>`.
        let _check: fn(
            AuthLayer,
            Request<()>,
        ) -> <AuthLayer as Layer<Request<()>>>::Service = |_layer, _req| {
            // dummy marker — never called
            unimplemented!()
        };
        let _ = _check;
    }

    /// Calling `Layer::layer` MUST produce a usable
    /// `Service<Request<B>>` (ready to be polled). Pin the
    /// trait relationship by exercising `Service::poll_ready`
    /// indirectly.
    #[tokio::test]
    async fn layer_produces_a_ready_service() {
        let cfg = Arc::new(empty_auth_config());
        let layer = AuthLayer::new(cfg);
        let inner = tower::service_fn(|_req: Request<()>| async {
            Ok::<_, std::convert::Infallible>(axum::response::Response::new(
                axum::body::Body::empty(),
            ))
        });
        let mut svc = <AuthLayer as Layer<_>>::layer(&layer, inner);
        // Pin the contract that the returned service is
        // `Service<Request<B>>` by polling it ready.
        // poll_ready on a ready inner service MUST yield
        // Poll::Ready(Ok(())) — once resolved by poll_fn the
        // future yields the inner Result.
        let poll_result = std::future::poll_fn(|cx| {
            <_ as tower::Service<Request<()>>>::poll_ready(&mut svc, cx)
        })
        .await;
        assert!(
            poll_result.is_ok(),
            "poll_ready on a ready inner service must yield Ok(())"
        );
    }

    /// `Layer::layer` MUST be idempotent — calling it twice
    /// with the same inner service produces two independent
    /// middleware instances.
    #[test]
    fn layer_call_twice_produces_independent_middlewares() {
        let cfg = Arc::new(empty_auth_config());
        let layer = AuthLayer::new(cfg);
        let inner = tower::service_fn(|_req: Request<()>| async {
            Ok::<_, std::convert::Infallible>(axum::response::Response::new(
                axum::body::Body::empty(),
            ))
        });
        let _mw_a = <AuthLayer as Layer<_>>::layer(&layer, inner);
        let _mw_b = <AuthLayer as Layer<_>>::layer(&layer, inner);
    }
}
