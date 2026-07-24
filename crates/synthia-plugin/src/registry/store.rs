//! The [`PluginRegistry`] struct plus its discovery, lifecycle, and
//! accessor methods.
//!
//! `PluginRegistry` is a thin wrapper around
//! `HashMap<PluginId, PluginHandle>`. It owns nothing but the map;
//! loading / parsing / validation lives in
//! [`PluginHandle::load`](super::handle::PluginHandle::load).

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use super::{
    handle::PluginHandle,
    types::{PluginId, PluginPath},
};
use crate::PluginError;

/// The plugin registry for managing loaded plugins
#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: HashMap<PluginId, PluginHandle>,
}

impl PluginRegistry {
    /// Create a new empty plugin registry
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Discover user plugins from ~/.synthia/plugins/
    pub fn discover_user_plugins() -> Result<Vec<PluginPath>, PluginError> {
        let plugin_dir = Self::user_plugins_dir()?;
        Self::discover_plugins_in_dir(&plugin_dir)
    }

    /// Discover project plugins from <project>/.synthia/plugins/
    pub fn discover_project_plugins(
        project_path: &Path,
    ) -> Result<Vec<PluginPath>, PluginError> {
        let plugin_dir = project_path.join(".synthia").join("plugins");
        Self::discover_plugins_in_dir(&plugin_dir)
    }

    /// Load a plugin from a path and add it to the registry
    pub fn load_plugin(
        &mut self,
        path: &PluginPath,
    ) -> Result<PluginHandle, PluginError> {
        let handle = PluginHandle::load(path)?;

        // Check for duplicate names
        for existing in self.plugins.values() {
            if existing.manifest.name == handle.manifest.name {
                return Err(PluginError::DuplicatePlugin(handle.manifest.name));
            }
        }

        let id = handle.id;
        self.plugins.insert(id, handle.clone());

        Ok(handle)
    }

    /// Unload a plugin by its ID
    pub fn unload_plugin(&mut self, id: &PluginId) -> Result<(), PluginError> {
        self.plugins
            .remove(id)
            .ok_or(PluginError::PluginNotLoaded(*id))?;
        Ok(())
    }

    /// Get a plugin by ID
    pub fn get(&self, id: &PluginId) -> Option<&PluginHandle> {
        self.plugins.get(id)
    }

    /// Get a plugin by name
    pub fn get_by_name(&self, name: &str) -> Option<&PluginHandle> {
        self.plugins.values().find(|h| h.manifest.name == name)
    }

    /// Get all loaded plugins
    pub fn all(&self) -> impl Iterator<Item = &PluginHandle> {
        self.plugins.values()
    }

    /// Get the number of loaded plugins
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Remove all plugins from the registry
    pub fn clear(&mut self) {
        self.plugins.clear();
    }

    /// Get the user plugins directory path
    pub(crate) fn user_plugins_dir() -> Result<PathBuf, PluginError> {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| PluginError::HomeDirectoryNotFound)?;

        Ok(home.join(".synthia").join("plugins"))
    }

    /// Scan a directory for plugins (directories containing plugin.json)
    pub(crate) fn discover_plugins_in_dir(
        dir: &Path,
    ) -> Result<Vec<PluginPath>, PluginError> {
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut plugins = Vec::new();
        let entries = fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() && path.join("plugin.json").exists() {
                plugins.push(PluginPath::new(path));
            }
        }

        Ok(plugins)
    }
}
