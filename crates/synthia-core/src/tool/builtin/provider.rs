//! BuiltinToolProvider — dual-map provider with immutable application tools
//! and shadowable local tools.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::tool::{
    descriptor::{Tool, ToolDescriptor, ToolProvenance},
    provider::{ToolCall, ToolEvent, ToolProvider},
    registry::RegistrationError,
    tool_name::ToolName,
    types::{ToolError, ToolOutput},
};

/// Inner state behind the RwLock.
struct BuiltinInner {
    /// Immutable core tools — registered once at startup.
    applications: HashMap<ToolName, Arc<dyn Tool>>,
    /// Runtime additions — may shadow application tools (LIFO).
    local: HashMap<ToolName, Arc<dyn Tool>>,
    /// Cached descriptors for application tools.
    application_descriptors: HashMap<ToolName, ToolDescriptor>,
    /// Cached descriptors for local tools.
    local_descriptors: HashMap<ToolName, ToolDescriptor>,
}

/// Provider that owns application (immutable) and local (shadowable) tools.
pub struct BuiltinToolProvider {
    inner: RwLock<BuiltinInner>,
}

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
                name: descriptor.name.clone(),
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
        let key =
            ToolName::parse(name).unwrap_or_else(|| ToolName::plain(name));
        let mut inner = self.inner.write();
        inner.local_descriptors.remove(&key);
        inner.local.remove(&key)
    }
}

impl Default for BuiltinToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

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
        let key =
            ToolName::parse(name).unwrap_or_else(|| ToolName::plain(name));
        let inner = self.inner.read();
        // LIFO: local shadows application.
        if let Some(tool) = inner.local.get(&key) {
            return Some(Arc::clone(tool));
        }
        inner.applications.get(&key).cloned()
    }

    async fn on_tool_event(&self, _event: &ToolEvent) {}

    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}
