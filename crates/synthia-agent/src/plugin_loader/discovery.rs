//! 3 private plugin discovery methods on
//! [`super::core::AgentPluginLoader`].
//!
//! - [`AgentPluginLoader::discover_user_plugins`] —
//!   reads `~/.synthia/plugins/` via
//!   [`PluginRegistry::discover_user_plugins`], loads each
//!   discovered plugin, then loads its hooks.
//! - [`AgentPluginLoader::discover_plugins_from`] —
//!   manual scan of a custom directory: each subdirectory
//!   containing `plugin.json` is loaded as a plugin.
//! - [`AgentPluginLoader::load_plugin_hooks`] — load
//!   `hooks.json` into the shared `HookRunner` (no-op if
//!   the file is absent).

use std::path::PathBuf;

use synthia_plugin::{PluginPath, PluginRegistry};

use super::{core::AgentPluginLoader, error::PluginLoaderError};

impl AgentPluginLoader {
    pub(in crate::plugin_loader) async fn discover_user_plugins(
        &mut self,
    ) -> Result<(), PluginLoaderError> {
        let plugin_paths = PluginRegistry::discover_user_plugins()
            .map_err(|e| PluginLoaderError::DiscoveryFailed(e.to_string()))?;

        self.plugins_dir = Some(Self::user_plugins_path()?);

        for path in plugin_paths {
            match self.registry.load_plugin(&path) {
                Ok(handle) => {
                    tracing::info!(
                        plugin = %handle.manifest.name,
                        version = %handle.manifest.version,
                        path = %path,
                        "Loaded plugin"
                    );

                    // Load hooks for this plugin
                    if let Err(e) = self.load_plugin_hooks(&path).await {
                        tracing::warn!(
                            plugin = %handle.manifest.name,
                            error = %e,
                            "Failed to load plugin hooks"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path,
                        error = %e,
                        "Failed to load plugin"
                    );
                }
            }
        }

        Ok(())
    }

    pub(in crate::plugin_loader) async fn discover_plugins_from(
        &mut self,
        dir: &PathBuf,
    ) -> Result<(), PluginLoaderError> {
        // Use discover_user_plugins logic for custom dir by scanning manually
        if !dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(dir)
            .map_err(|e| PluginLoaderError::DiscoveryFailed(e.to_string()))?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to read directory entry");
                    continue;
                }
            };

            let path = entry.path();
            if path.is_dir() && path.join("plugin.json").exists() {
                let plugin_path = PluginPath::new(path.clone());
                match self.registry.load_plugin(&plugin_path) {
                    Ok(handle) => {
                        tracing::info!(
                            plugin = %handle.manifest.name,
                            version = %handle.manifest.version,
                            path = %plugin_path,
                            "Loaded plugin from directory"
                        );

                        if let Err(e) =
                            self.load_plugin_hooks(&plugin_path).await
                        {
                            tracing::warn!(
                                plugin = %handle.manifest.name,
                                error = %e,
                                "Failed to load plugin hooks"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %plugin_path,
                            error = %e,
                            "Failed to load plugin"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub(in crate::plugin_loader) async fn load_plugin_hooks(
        &self,
        path: &PluginPath,
    ) -> Result<(), PluginLoaderError> {
        let hooks_path = path.hooks_path();
        if !hooks_path.exists() {
            return Ok(());
        }

        let mut runner = self.hook_runner.lock().await;
        runner
            .load_from_path(path.as_path())
            .map_err(|e| PluginLoaderError::HookLoadFailed(e.to_string()))?;

        Ok(())
    }
}
