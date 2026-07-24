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
