//! The [`AgentPluginLoader`] struct + 2 constructors
//! (`new` / `with_plugins_dir`) + 5 accessor methods +
//! Debug impl.
//!
//! The 3 private discovery methods live in
//! [`super::discovery`] and the 4 hook-fire methods live
//! in [`super::fire`].

use std::{path::PathBuf, sync::Arc};

use synthia_plugin::{HookRunner, PluginRegistry, SharedHookRunner};
use tokio::sync::Mutex;

use super::error::PluginLoaderError;

/// Agent plugin loader that manages plugin lifecycle and hook integration.
pub struct AgentPluginLoader {
    /// Plugin registry for plugin discovery and lifecycle
    pub(in crate::plugin_loader) registry: PluginRegistry,
    /// Hook runner for executing plugin hooks
    pub(in crate::plugin_loader) hook_runner: SharedHookRunner,
    /// User plugins directory path
    pub(in crate::plugin_loader) plugins_dir: Option<PathBuf>,
}

impl AgentPluginLoader {
    /// Create a new plugin loader and discover user plugins.
    pub async fn new() -> Result<Self, PluginLoaderError> {
        let mut loader = Self {
            registry: PluginRegistry::new(),
            hook_runner: Arc::new(Mutex::new(HookRunner::new())),
            plugins_dir: None,
        };

        // Attempt to discover and load user plugins
        if let Err(e) = loader.discover_user_plugins().await {
            tracing::warn!(error = %e, "Failed to discover user plugins, continuing without plugins");
        }

        Ok(loader)
    }

    /// Create a new plugin loader with a custom plugins directory.
    pub async fn with_plugins_dir(
        plugins_dir: PathBuf,
    ) -> Result<Self, PluginLoaderError> {
        let mut loader = Self {
            registry: PluginRegistry::new(),
            hook_runner: Arc::new(Mutex::new(HookRunner::new())),
            plugins_dir: Some(plugins_dir.clone()),
        };

        // Discover plugins from custom directory
        if plugins_dir.exists()
            && let Err(e) = loader.discover_plugins_from(&plugins_dir).await
        {
            tracing::warn!(
                error = %e,
                path = %plugins_dir.display(),
                "Failed to discover plugins from directory"
            );
        }

        Ok(loader)
    }

    /// Get the shared hook runner for external access.
    pub fn hook_runner(&self) -> SharedHookRunner {
        Arc::clone(&self.hook_runner)
    }

    /// Get the number of loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.registry.len()
    }

    /// Get a plugin by name.
    pub fn get_plugin(
        &self,
        name: &str,
    ) -> Option<&synthia_plugin::PluginHandle> {
        self.registry.get_by_name(name)
    }

    /// Get all loaded plugins.
    pub fn all_plugins(
        &self,
    ) -> impl Iterator<Item = &synthia_plugin::PluginHandle> {
        self.registry.all()
    }

    /// Get the path to the user plugins directory.
    pub(in crate::plugin_loader) fn user_plugins_path()
    -> Result<PathBuf, PluginLoaderError> {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| PluginLoaderError::HomeDirectoryNotFound)?;

        Ok(home.join(".synthia").join("plugins"))
    }
}

impl std::fmt::Debug for AgentPluginLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentPluginLoader")
            .field("plugin_count", &self.registry.len())
            .field("plugins_dir", &self.plugins_dir)
            .finish()
    }
}
