//! Extension manager for dynamic tool providers.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use dashmap::DashMap;

use super::tool_provider::{ToolDefinition, ToolProvider};

/// Manages dynamic tool providers with O(1) cache invalidation.
#[derive(Clone)]
pub struct ExtensionManager {
    providers: Arc<DashMap<String, Arc<dyn ToolProvider>>>,
    cache_version: Arc<AtomicU64>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
            cache_version: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Register a provider. Overwrites any existing provider with the same name.
    /// Increments the cache version, invalidating all cached tool lists.
    pub fn register(&self, provider: Arc<dyn ToolProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
        self.cache_version.fetch_add(1, Ordering::SeqCst);
    }

    /// Unregister a provider by name. Returns `true` if a provider was removed.
    pub fn unregister(&self, name: &str) -> bool {
        let removed = self.providers.remove(name).is_some();
        if removed {
            self.cache_version.fetch_add(1, Ordering::SeqCst);
        }
        removed
    }

    /// List all tools from all registered providers. O(n) over all providers.
    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        self.providers
            .iter()
            .flat_map(|entry| entry.value().list_tools())
            .collect()
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolProvider>> {
        self.providers.get(name).map(|e| e.value().clone())
    }

    /// Get the current cache version. Incremented on any registration change.
    pub fn cache_version(&self) -> u64 {
        self.cache_version.load(Ordering::SeqCst)
    }

    /// Return `true` if no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for ExtensionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::tools::dynamic_provider::tool_provider::SchemaRef;

    struct DummyProvider;

    #[async_trait]
    impl ToolProvider for DummyProvider {
        fn name(&self) -> &'static str {
            "dummy"
        }

        fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "dummy_tool".to_string(),
                description: "A dummy tool".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {},
                })),
                deprecated: None,
            }]
        }
    }

    #[tokio::test]
    async fn register_and_list() {
        let manager = ExtensionManager::new();
        manager.register(Arc::new(DummyProvider));
        let tools = manager.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "dummy_tool");
    }

    #[tokio::test]
    async fn unregister() {
        let manager = ExtensionManager::new();
        manager.register(Arc::new(DummyProvider));
        assert!(manager.unregister("dummy"));
        assert!(!manager.unregister("nonexistent"));
        assert!(manager.is_empty());
    }

    #[tokio::test]
    async fn cache_version_increments_on_register() {
        let manager = ExtensionManager::new();
        let v0 = manager.cache_version();
        manager.register(Arc::new(DummyProvider));
        let v1 = manager.cache_version();
        assert!(v1 > v0);
    }
}
