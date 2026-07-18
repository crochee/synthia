//! BuiltinToolProvider — dual-map provider with immutable application tools
//! and shadowable local tools.

#[cfg(feature = "unified-registry")]
use std::{collections::HashMap, sync::Arc};

#[cfg(feature = "unified-registry")]
use async_trait::async_trait;
#[cfg(feature = "unified-registry")]
use parking_lot::RwLock;

#[cfg(feature = "unified-registry")]
use crate::tool::{
    descriptor::{Tool, ToolDescriptor, ToolProvenance},
    provider::{ToolCall, ToolEvent, ToolProvider},
    registry::RegistrationError,
    types::{ToolError, ToolOutput},
};

/// Inner state behind the RwLock.
#[cfg(feature = "unified-registry")]
struct BuiltinInner {
    /// Immutable core tools — registered once at startup.
    applications: HashMap<String, Arc<dyn Tool>>,
    /// Runtime additions — may shadow application tools (LIFO).
    local: HashMap<String, Arc<dyn Tool>>,
    /// Cached descriptors for application tools.
    application_descriptors: HashMap<String, ToolDescriptor>,
    /// Cached descriptors for local tools.
    local_descriptors: HashMap<String, ToolDescriptor>,
}

/// Provider that owns application (immutable) and local (shadowable) tools.
#[cfg(feature = "unified-registry")]
pub struct BuiltinToolProvider {
    inner: RwLock<BuiltinInner>,
}

#[cfg(feature = "unified-registry")]
impl BuiltinToolProvider {
    /// Create an empty provider.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(BuiltinInner {
                applications: HashMap::new(),
                local: HashMap::new(),
                application_descriptors: HashMap::new(),
                local_descriptors: HashMap::new(),
            }),
        }
    }

    /// Register an immutable application tool.
    ///
    /// Returns `CoreNameTaken` if a tool with the same name already exists
    /// in the application map and the descriptor provenance is `Core`.
    pub fn register_builtin(
        &mut self,
        tool: Arc<dyn Tool>,
        descriptor: ToolDescriptor,
    ) -> Result<(), RegistrationError> {
        let mut inner = self.inner.write();
        if descriptor.provenance == ToolProvenance::Core
            && inner.applications.contains_key(&descriptor.name)
        {
            return Err(RegistrationError::CoreNameTaken {
                name: descriptor.name,
            });
        }
        let name = descriptor.name.clone();
        inner.applications.insert(name.clone(), tool);
        inner.application_descriptors.insert(name, descriptor);
        Ok(())
    }

    /// Add a local (runtime) tool. Allows shadowing — the local tool
    /// takes precedence over any application tool with the same name.
    pub fn add_local(
        &mut self,
        tool: Arc<dyn Tool>,
        descriptor: ToolDescriptor,
    ) {
        let mut inner = self.inner.write();
        let name = descriptor.name.clone();
        inner.local.insert(name.clone(), tool);
        inner.local_descriptors.insert(name, descriptor);
    }

    /// Remove a local tool by name. Returns the removed tool, if any.
    pub fn remove_local(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        let mut inner = self.inner.write();
        inner.local_descriptors.remove(name);
        inner.local.remove(name)
    }
}

#[cfg(feature = "unified-registry")]
impl Default for BuiltinToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "unified-registry")]
#[async_trait]
impl ToolProvider for BuiltinToolProvider {
    fn id(&self) -> &str {
        "builtin"
    }

    async fn list_tools(&self) -> Vec<ToolDescriptor> {
        let inner = self.inner.read();
        let mut descs: Vec<ToolDescriptor> =
            inner.application_descriptors.values().cloned().collect();
        descs.extend(inner.local_descriptors.values().cloned());
        descs
    }

    async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let inner = self.inner.read();
        // LIFO: local shadows application.
        if let Some(tool) = inner.local.get(name) {
            return Some(Arc::clone(tool));
        }
        inner.applications.get(name).cloned()
    }

    async fn on_tool_event(&self, _event: &ToolEvent) {}

    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}
