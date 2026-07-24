//! `AgentRegistry` struct, builders, `Default`, and `Clone` impls.
//!
//! The struct fields are `pub(super)` so the `load` /
//! `instances` / `query` / `registry_trait` submodules can
//! manipulate state directly.

use std::sync::Arc;

use indexmap::IndexMap;
use parking_lot::RwLock;
use synthia_tool::registry::ToolRegistry;

use crate::registry::instance::AgentInstance;

/// Holds agent [`AgentDefinition`]s (read-only after load) and
/// a live set of [`AgentInstance`]s (mutable, lifecycle
/// managed by [`super::instances`]).
///
/// `Clone` semantics are interesting: cloning an
/// `AgentRegistry` copies the definitions and config but NOT
/// the running instances — the clone starts empty, so two
/// `AgentRegistry` instances never share mutable instance
/// state.
pub struct AgentRegistry {
    pub(super) definitions:
        RwLock<IndexMap<String, super::types::AgentDefinition>>,
    pub(super) instances:
        RwLock<IndexMap<String, Arc<std::sync::Mutex<AgentInstance>>>>,
    pub(super) max_depth: usize,
    pub(super) tool_registry: Option<Arc<ToolRegistry>>,
}

impl AgentRegistry {
    /// Create a new empty registry with `max_depth = 1` and no
    /// tool registry.
    pub fn new() -> Self {
        Self {
            definitions: RwLock::new(IndexMap::new()),
            instances: RwLock::new(IndexMap::new()),
            max_depth: 1,
            tool_registry: None,
        }
    }

    /// Attach a [`ToolRegistry`] so that spawned agent
    /// instances can resolve tool calls.
    pub fn with_tool_registry(mut self, registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    /// Set the max agent-spawn depth. Spawning deeper than
    /// this fails with [`synthia_core::Error::InvalidItem`].
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }
}

impl Clone for AgentRegistry {
    fn clone(&self) -> Self {
        Self {
            definitions: RwLock::new(self.definitions.read().clone()),
            instances: RwLock::new(IndexMap::new()),
            max_depth: self.max_depth,
            tool_registry: self.tool_registry.clone(),
        }
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
