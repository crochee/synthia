//! PluginRegistry — dynamic discovery and loading of third-party extension packages.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Plugin capability summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilitySummary {
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub fragments: Vec<String>,
}

/// Plugin trait.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// Plugin ID (unique).
    fn id(&self) -> &str;
    /// Plugin version.
    fn version(&self) -> &str;
    /// Plugin description.
    fn description(&self) -> &str;
    /// Plugin capability summary.
    fn capabilities(&self) -> PluginCapabilitySummary;
    /// Initialize the plugin (register tools, skills, fragments, etc.).
    async fn initialize(&self) -> Result<(), PluginError>;
    /// Shutdown the plugin (cleanup resources).
    async fn shutdown(&self) -> Result<(), PluginError>;
}

/// Plugin error type.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),
    #[error("Plugin initialization failed: {0}")]
    InitFailed(String),
    #[error("Plugin shutdown failed: {0}")]
    ShutdownFailed(String),
    #[error("Plugin discovery failed: {0}")]
    DiscoveryFailed(String),
}

/// Plugin state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    Discovered,
    Loaded,
    Initialized,
    Failed,
    Unloaded,
}

/// Registry for dynamic discovery and loading of third-party plugin packages.
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, Arc<dyn Plugin>>>,
    states: RwLock<HashMap<String, PluginState>>,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
        }
    }

    /// Load a plugin into the registry.
    pub async fn load(
        &self,
        plugin: Arc<dyn Plugin>,
    ) -> Result<(), PluginError> {
        let id = plugin.id().to_string();
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&id) {
                return Err(PluginError::AlreadyLoaded(id));
            }
        }
        self.plugins.write().await.insert(id.clone(), plugin);
        self.states.write().await.insert(id, PluginState::Loaded);
        Ok(())
    }

    /// Initialize a loaded plugin by ID.
    pub async fn initialize(&self, id: &str) -> Result<(), PluginError> {
        let plugin = {
            let plugins = self.plugins.read().await;
            plugins
                .get(id)
                .cloned()
                .ok_or_else(|| PluginError::NotFound(id.to_string()))?
        };
        if let Err(e) = plugin.initialize().await {
            self.states
                .write()
                .await
                .insert(id.to_string(), PluginState::Failed);
            return Err(PluginError::InitFailed(e.to_string()));
        }
        self.states
            .write()
            .await
            .insert(id.to_string(), PluginState::Initialized);
        Ok(())
    }

    /// Initialize all loaded but not yet initialized plugins.
    pub async fn initialize_all(&self) -> Vec<Result<(), PluginError>> {
        let ids: Vec<String> = {
            let states = self.states.read().await;
            states
                .iter()
                .filter(|(_, state)| **state == PluginState::Loaded)
                .map(|(id, _)| id.clone())
                .collect()
        };
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            results.push(self.initialize(&id).await);
        }
        results
    }

    /// Unload a plugin (call shutdown, then remove).
    pub async fn unload(&self, id: &str) -> Result<(), PluginError> {
        let plugin = {
            let plugins = self.plugins.read().await;
            plugins
                .get(id)
                .cloned()
                .ok_or_else(|| PluginError::NotFound(id.to_string()))?
        };
        if let Err(e) = plugin.shutdown().await {
            self.states
                .write()
                .await
                .insert(id.to_string(), PluginState::Failed);
            return Err(PluginError::ShutdownFailed(e.to_string()));
        }
        self.plugins.write().await.remove(id);
        self.states.write().await.remove(id);
        Ok(())
    }

    /// Get a plugin by ID.
    pub async fn get(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins.read().await.get(id).cloned()
    }

    /// Get the state of a plugin by ID.
    pub async fn state(&self, id: &str) -> Option<PluginState> {
        self.states.read().await.get(id).copied()
    }

    /// List all plugin IDs.
    pub async fn list(&self) -> Vec<String> {
        self.plugins.read().await.keys().cloned().collect()
    }

    /// Get capability summaries for all plugins.
    pub async fn capability_summaries(
        &self,
    ) -> HashMap<String, PluginCapabilitySummary> {
        let plugins = self.plugins.read().await;
        plugins
            .iter()
            .map(|(id, p)| (id.clone(), p.capabilities()))
            .collect()
    }

    /// Get the number of plugins.
    pub async fn plugin_count(&self) -> usize {
        self.plugins.read().await.len()
    }

    /// Shutdown all plugins.
    pub async fn shutdown_all(&self) -> Vec<Result<(), PluginError>> {
        let ids: Vec<String> =
            self.plugins.read().await.keys().cloned().collect();
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            let plugin = {
                let plugins = self.plugins.read().await;
                plugins.get(&id).cloned()
            };
            if let Some(plugin) = plugin {
                if let Err(e) = plugin.shutdown().await {
                    self.states
                        .write()
                        .await
                        .insert(id.clone(), PluginState::Failed);
                    results
                        .push(Err(PluginError::ShutdownFailed(e.to_string())));
                } else {
                    self.states
                        .write()
                        .await
                        .insert(id.clone(), PluginState::Unloaded);
                    results.push(Ok(()));
                }
            }
        }
        results
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple test plugin for unit testing.
    struct TestPlugin {
        id: String,
        version: String,
        description: String,
        capabilities: PluginCapabilitySummary,
    }

    impl TestPlugin {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                version: "0.1.0".to_string(),
                description: format!("Test plugin {id}"),
                capabilities: PluginCapabilitySummary {
                    tools: vec![format!("{id}:tool1")],
                    skills: vec![format!("{id}:skill1")],
                    fragments: vec![format!("{id}:fragment1")],
                },
            }
        }
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn id(&self) -> &str {
            &self.id
        }

        fn version(&self) -> &str {
            &self.version
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn capabilities(&self) -> PluginCapabilitySummary {
            self.capabilities.clone()
        }

        async fn initialize(&self) -> Result<(), PluginError> {
            Ok(())
        }

        async fn shutdown(&self) -> Result<(), PluginError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn new_registry_is_empty() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.plugin_count().await, 0);
        assert!(registry.list().await.is_empty());
    }

    #[tokio::test]
    async fn load_plugin_adds_to_registry() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin::new("test-plugin"));
        let result = registry.load(plugin).await;
        assert!(result.is_ok());
        assert_eq!(registry.plugin_count().await, 1);
        assert_eq!(
            registry.state("test-plugin").await,
            Some(PluginState::Loaded)
        );
    }

    #[tokio::test]
    async fn load_duplicate_returns_error() {
        let registry = PluginRegistry::new();
        let plugin1 = Arc::new(TestPlugin::new("dup-plugin"));
        let plugin2 = Arc::new(TestPlugin::new("dup-plugin"));
        assert!(registry.load(plugin1).await.is_ok());
        let result = registry.load(plugin2).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::AlreadyLoaded(id) => assert_eq!(id, "dup-plugin"),
            other => panic!("Expected AlreadyLoaded, got: {other}"),
        }
    }

    #[tokio::test]
    async fn initialize_plugin() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin::new("init-plugin"));
        registry.load(plugin).await.unwrap();
        let result = registry.initialize("init-plugin").await;
        assert!(result.is_ok());
        assert_eq!(
            registry.state("init-plugin").await,
            Some(PluginState::Initialized)
        );
    }

    #[tokio::test]
    async fn initialize_nonexistent_returns_error() {
        let registry = PluginRegistry::new();
        let result = registry.initialize("no-such-plugin").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::NotFound(id) => assert_eq!(id, "no-such-plugin"),
            other => panic!("Expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn unload_plugin_removes_from_registry() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin::new("unload-plugin"));
        registry.load(plugin).await.unwrap();
        let result = registry.unload("unload-plugin").await;
        assert!(result.is_ok());
        assert_eq!(registry.plugin_count().await, 0);
        assert!(registry.get("unload-plugin").await.is_none());
        assert!(registry.state("unload-plugin").await.is_none());
    }

    #[tokio::test]
    async fn unload_nonexistent_returns_error() {
        let registry = PluginRegistry::new();
        let result = registry.unload("ghost-plugin").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::NotFound(id) => assert_eq!(id, "ghost-plugin"),
            other => panic!("Expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn list_returns_all_ids() {
        let registry = PluginRegistry::new();
        registry.load(Arc::new(TestPlugin::new("a"))).await.unwrap();
        registry.load(Arc::new(TestPlugin::new("b"))).await.unwrap();
        registry.load(Arc::new(TestPlugin::new("c"))).await.unwrap();
        let mut ids = registry.list().await;
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn capability_summaries() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("cap-plugin")))
            .await
            .unwrap();
        let summaries = registry.capability_summaries().await;
        assert!(summaries.contains_key("cap-plugin"));
        let summary = &summaries["cap-plugin"];
        assert_eq!(summary.tools, vec!["cap-plugin:tool1"]);
        assert_eq!(summary.skills, vec!["cap-plugin:skill1"]);
        assert_eq!(summary.fragments, vec!["cap-plugin:fragment1"]);
    }

    #[tokio::test]
    async fn plugin_count() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.plugin_count().await, 0);
        registry
            .load(Arc::new(TestPlugin::new("p1")))
            .await
            .unwrap();
        assert_eq!(registry.plugin_count().await, 1);
        registry
            .load(Arc::new(TestPlugin::new("p2")))
            .await
            .unwrap();
        assert_eq!(registry.plugin_count().await, 2);
        registry.unload("p1").await.unwrap();
        assert_eq!(registry.plugin_count().await, 1);
    }

    #[tokio::test]
    async fn initialize_all_initializes_loaded_plugins() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("ia1")))
            .await
            .unwrap();
        registry
            .load(Arc::new(TestPlugin::new("ia2")))
            .await
            .unwrap();
        let results = registry.initialize_all().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(registry.state("ia1").await, Some(PluginState::Initialized));
        assert_eq!(registry.state("ia2").await, Some(PluginState::Initialized));
    }

    #[tokio::test]
    async fn shutdown_all_shuts_down_all_plugins() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("sa1")))
            .await
            .unwrap();
        registry
            .load(Arc::new(TestPlugin::new("sa2")))
            .await
            .unwrap();
        let results = registry.shutdown_all().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(registry.state("sa1").await, Some(PluginState::Unloaded));
        assert_eq!(registry.state("sa2").await, Some(PluginState::Unloaded));
    }

    #[tokio::test]
    async fn get_returns_plugin() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("get-plugin")))
            .await
            .unwrap();
        let plugin = registry.get("get-plugin").await;
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().id(), "get-plugin");
        assert!(registry.get("nonexistent").await.is_none());
    }
}
