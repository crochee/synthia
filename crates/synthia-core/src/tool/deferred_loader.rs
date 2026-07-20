//! Deferred tool loading mechanism for ToolExposure::Deferred tools.
//!
//! When a tool has `ToolExposure::Deferred`, only its name and description
//! are sent to the LLM initially. The full schema is loaded on first invocation
//! via a [`DeferredToolLoader`].

use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;

use crate::tool::{
    descriptor::ToolDescriptor,
    registry::ToolRegistry,
    tool_name::ToolName,
};

/// Error type for deferred loading operations.
#[derive(Debug, thiserror::Error)]
pub enum DeferredLoadError {
    /// The requested tool was not found in the loader's pending set.
    #[error("Tool not found in registry: {0}")]
    NotFound(String),
    /// The tool has already been materialized and cannot be loaded again.
    #[error("Tool already materialized: {0}")]
    AlreadyMaterialized(String),
    /// The loading operation failed for an arbitrary reason.
    #[error("Loading failed: {0}")]
    LoadFailed(String),
}

/// A loader that can resolve a deferred tool's full definition on demand.
///
/// Called when a deferred tool is first invoked. The loader should
/// resolve the complete tool definition (parameters, examples, etc.)
/// and register it in the provided registry.
#[async_trait]
pub trait DeferredToolLoader: Send + Sync + 'static {
    /// Load the full tool definition for a deferred tool.
    ///
    /// Returns the complete [`ToolDescriptor`] including the full
    /// parameters schema, examples, and all metadata.
    async fn load(
        &self,
        name: &ToolName,
        registry: &ToolRegistry,
    ) -> Result<ToolDescriptor, DeferredLoadError>;

    /// Check if a deferred tool can be loaded by this loader.
    fn can_load(&self, name: &ToolName) -> bool;
}

/// A simple loader that resolves deferred tools from a backing store
/// (e.g., a `HashMap` of pre-registered but not yet exposed tools).
///
/// This is the default implementation suitable for most use cases
/// where tool definitions are known upfront but should not be fully
/// exposed to the LLM until first invocation.
pub struct SimpleDeferredLoader {
    /// Tools that are available but not yet fully materialized.
    pending: RwLock<HashMap<ToolName, ToolDescriptor>>,
}

impl SimpleDeferredLoader {
    /// Create a new empty loader.
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
        }
    }

    /// Register a pending deferred tool definition.
    pub fn add_pending(&self, descriptor: ToolDescriptor) {
        let mut pending = self.pending.write().unwrap();
        pending.insert(descriptor.name.clone(), descriptor);
    }

    /// Remove a pending tool.
    pub fn remove_pending(&self, name: &ToolName) -> Option<ToolDescriptor> {
        let mut pending = self.pending.write().unwrap();
        pending.remove(name)
    }

    /// Number of pending tools.
    pub fn pending_count(&self) -> usize {
        self.pending.read().unwrap().len()
    }
}

impl Default for SimpleDeferredLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DeferredToolLoader for SimpleDeferredLoader {
    async fn load(
        &self,
        name: &ToolName,
        _registry: &ToolRegistry,
    ) -> Result<ToolDescriptor, DeferredLoadError> {
        let pending = self.pending.read().unwrap();
        pending
            .get(name)
            .cloned()
            .ok_or_else(|| DeferredLoadError::NotFound(name.to_string()))
    }

    fn can_load(&self, name: &ToolName) -> bool {
        self.pending.read().unwrap().contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::descriptor::{
        CancelBehavior,
        ExecutionMode,
        ToolCategory,
        ToolExposure,
        ToolProvenance,
    };

    /// Helper: create a minimal `ToolDescriptor` for testing.
    fn make_descriptor(name: &str) -> ToolDescriptor {
        ToolDescriptor {
            name: ToolName::plain(name),
            description: format!("{name} description"),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "arg1": { "type": "string" }
                }
            }),
            category: ToolCategory::Utility,
            provenance: ToolProvenance::Core,
            execution_mode: ExecutionMode::Parallel,
            cancel_behavior: CancelBehavior::Cooperative,
            examples: vec![],
            permission_required: false,
            prompt_visible_provenance: true,
            is_hidden: false,
            is_user_invocable: true,
            exposure: ToolExposure::Deferred,
        }
    }

    #[test]
    fn new_creates_empty_loader() {
        let loader = SimpleDeferredLoader::new();
        assert_eq!(loader.pending_count(), 0);
    }

    #[test]
    fn default_trait_works() {
        let loader = SimpleDeferredLoader::default();
        assert_eq!(loader.pending_count(), 0);
    }

    #[test]
    fn add_pending_increments_count() {
        let loader = SimpleDeferredLoader::new();
        loader.add_pending(make_descriptor("tool-a"));
        assert_eq!(loader.pending_count(), 1);

        loader.add_pending(make_descriptor("tool-b"));
        assert_eq!(loader.pending_count(), 2);
    }

    #[test]
    fn add_pending_overwrites_duplicate() {
        let loader = SimpleDeferredLoader::new();
        loader.add_pending(make_descriptor("tool-a"));
        loader.add_pending(make_descriptor("tool-a")); // same name, overwrites
        assert_eq!(loader.pending_count(), 1);
    }

    #[test]
    fn remove_pending_decrements_count() {
        let loader = SimpleDeferredLoader::new();
        loader.add_pending(make_descriptor("tool-a"));
        loader.add_pending(make_descriptor("tool-b"));
        assert_eq!(loader.pending_count(), 2);

        let removed = loader.remove_pending(&ToolName::plain("tool-a"));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, ToolName::plain("tool-a"));
        assert_eq!(loader.pending_count(), 1);
    }

    #[test]
    fn remove_pending_returns_none_for_unknown() {
        let loader = SimpleDeferredLoader::new();
        let result = loader.remove_pending(&ToolName::plain("nonexistent"));
        assert!(result.is_none());
        assert_eq!(loader.pending_count(), 0);
    }

    #[test]
    fn pending_count_returns_correct_count() {
        let loader = SimpleDeferredLoader::new();
        assert_eq!(loader.pending_count(), 0);

        loader.add_pending(make_descriptor("a"));
        assert_eq!(loader.pending_count(), 1);

        loader.add_pending(make_descriptor("b"));
        loader.add_pending(make_descriptor("c"));
        assert_eq!(loader.pending_count(), 3);

        loader.remove_pending(&ToolName::plain("b"));
        assert_eq!(loader.pending_count(), 2);
    }

    #[test]
    fn can_load_returns_true_for_pending() {
        let loader = SimpleDeferredLoader::new();
        loader.add_pending(make_descriptor("my-tool"));

        assert!(loader.can_load(&ToolName::plain("my-tool")));
        assert!(!loader.can_load(&ToolName::plain("unknown-tool")));
    }

    #[test]
    fn can_load_returns_false_for_unknown() {
        let loader = SimpleDeferredLoader::new();
        assert!(!loader.can_load(&ToolName::plain("anything")));
    }

    #[tokio::test]
    async fn load_returns_descriptor_for_pending_tool() {
        let loader = SimpleDeferredLoader::new();
        let desc = make_descriptor("deferred-tool");
        loader.add_pending(desc.clone());

        let registry = ToolRegistry::new();
        let result = loader
            .load(&ToolName::plain("deferred-tool"), &registry)
            .await;
        assert!(result.is_ok());

        let loaded = result.unwrap();
        assert_eq!(loaded.name, ToolName::plain("deferred-tool"));
        assert_eq!(loaded.description, "deferred-tool description");
        // Full parameters should be present
        assert!(
            loaded
                .parameters
                .as_object()
                .unwrap()
                .contains_key("properties")
        );
    }

    #[tokio::test]
    async fn load_returns_not_found_for_unknown_tool() {
        let loader = SimpleDeferredLoader::new();
        let registry = ToolRegistry::new();

        let result = loader.load(&ToolName::plain("missing"), &registry).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            DeferredLoadError::NotFound(name) => {
                assert_eq!(name, "missing");
            }
            other => panic!("Expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn load_after_remove_returns_not_found() {
        let loader = SimpleDeferredLoader::new();
        loader.add_pending(make_descriptor("ephemeral"));

        let registry = ToolRegistry::new();
        // Load works before removal
        let result =
            loader.load(&ToolName::plain("ephemeral"), &registry).await;
        assert!(result.is_ok());

        // Remove the pending tool
        loader.remove_pending(&ToolName::plain("ephemeral"));

        // Now load should fail
        let result =
            loader.load(&ToolName::plain("ephemeral"), &registry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn can_load_false_after_remove() {
        let loader = SimpleDeferredLoader::new();
        loader.add_pending(make_descriptor("temp-tool"));
        assert!(loader.can_load(&ToolName::plain("temp-tool")));

        loader.remove_pending(&ToolName::plain("temp-tool"));
        assert!(!loader.can_load(&ToolName::plain("temp-tool")));
    }
}
