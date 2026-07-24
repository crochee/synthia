//! The [`McpProxy`] struct + `Default` + `Drop` + the 3
//! constructor methods (`new` / `default` /
//! `with_startup_timeout`).
//!
//! The 5 start methods live in [`super::start`]; the 5
//! stop/status methods live in [`super::stop`].

use std::{collections::HashMap, sync::Arc};

use tokio::{sync::RwLock, time::Duration};
use tracing::debug;

use super::handle::ServerHandle;
use crate::registry::McpServerConfig;

pub struct McpProxy {
    /// Managed servers by name
    pub(super) servers: Arc<RwLock<HashMap<String, ServerHandle>>>,
    /// Default startup timeout for servers
    pub(super) startup_timeout: Duration,
}

impl McpProxy {
    /// Create a new McpProxy with the given server configurations
    ///
    /// Servers are NOT started automatically - call `start_all()` to start them.
    pub fn new(configs: Vec<McpServerConfig>) -> Self {
        debug!("creating MCP proxy with {} server configs", configs.len());
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            startup_timeout: Duration::from_secs(30),
        }
    }

    /// Create a new McpProxy with default configuration
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(Vec::new())
    }

    /// Set the startup timeout for servers
    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }
}

impl Default for McpProxy {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Drop for McpProxy {
    fn drop(&mut self) {
        // Note: We can't do async cleanup in Drop
        // Users should call stop_all() explicitly or use a runtime
        debug!("McpProxy dropped - call stop_all() to cleanup servers");
    }
}
