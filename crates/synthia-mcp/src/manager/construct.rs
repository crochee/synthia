//! Constructors and the [`Default`] impl for [`McpManager`].
//!
//! Consolidates the four near-identical `new` / `with_*` constructors
//! from the original monolithic file behind a single
//! [`new_with_settings`](McpManager::new_with_settings) helper that all
//! other constructors delegate to.

use std::{path::PathBuf, sync::Arc, time::Duration};

use super::types::McpManager;
use crate::server::IdleTimeoutConfig;

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpManager {
    /// Common constructor used by all `new` / `with_*` entry points.
    /// Lets every public constructor stay a 1-3 line delegation while
    /// keeping the field-init shape in one place.
    fn new_with_settings(
        idle_config: IdleTimeoutConfig,
        credential_store: Arc<crate::oauth::CredentialStore>,
        hybrid_mode_enabled: bool,
        idle_timeout: Duration,
        cleanup_interval: Duration,
    ) -> Self {
        Self {
            connections: tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            ),
            configs: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            last_activity: tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            ),
            idle_config,
            credential_store,
            hybrid_mode_enabled,
            idle_timeout,
            cleanup_interval,
            discovered_tools: tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            ),
        }
    }

    pub fn new() -> Self {
        let idle_config = IdleTimeoutConfig::default();
        let credential_store = Arc::new(crate::oauth::CredentialStore::new(
            PathBuf::from(".synthia"),
        ));
        Self::new_with_settings(
            idle_config,
            credential_store,
            false,
            Duration::from_secs(300),
            Duration::from_secs(60),
        )
    }

    /// Create a new manager with custom idle timeout configuration.
    pub fn with_idle_config(idle_config: IdleTimeoutConfig) -> Self {
        let credential_store = Arc::new(crate::oauth::CredentialStore::new(
            PathBuf::from(".synthia"),
        ));
        Self::new_with_settings(
            idle_config,
            credential_store,
            false,
            Duration::from_secs(300),
            Duration::from_secs(60),
        )
    }

    /// Create a new manager with a shared credential store.
    pub fn with_credential_store(
        idle_config: IdleTimeoutConfig,
        store: Arc<crate::oauth::CredentialStore>,
    ) -> Self {
        Self::new_with_settings(
            idle_config,
            store,
            false,
            Duration::from_secs(300),
            Duration::from_secs(60),
        )
    }

    /// Create a new manager with hybrid mode enabled.
    pub fn with_hybrid_mode(enabled: bool) -> Self {
        let mut manager = Self::new();
        manager.hybrid_mode_enabled = enabled;
        manager
    }

    /// Create a new manager with hybrid mode and custom idle timeout.
    pub fn with_hybrid_mode_and_idle_timeout(
        enabled: bool,
        idle_timeout: Duration,
        cleanup_interval: Duration,
    ) -> Self {
        let idle_config = IdleTimeoutConfig::default();
        let credential_store = Arc::new(crate::oauth::CredentialStore::new(
            PathBuf::from(".synthia"),
        ));
        Self::new_with_settings(
            idle_config,
            credential_store,
            enabled,
            idle_timeout,
            cleanup_interval,
        )
    }
}
