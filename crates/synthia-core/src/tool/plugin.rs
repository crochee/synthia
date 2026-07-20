//! PluginToolProvider — namespaced plugin tools.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::tool::{
    descriptor::{Tool, ToolDescriptor, ToolProvenance},
    provider::{ToolCall, ToolEvent, ToolProvider},
    tool_name::ToolName,
    types::{ToolError, ToolOutput},
};

/// Tool provider backed by a plugin. All tool names are prefixed
/// with `plugin:<plugin_id>:` when `prompt_visible_provenance` is true.
pub struct PluginToolProvider {
    plugin_id: String,
    prompt_visible_provenance: bool,
    tools: HashMap<ToolName, Arc<dyn Tool>>,
    descriptors: HashMap<ToolName, ToolDescriptor>,
}

impl PluginToolProvider {
    pub fn new(plugin_id: String, prompt_visible_provenance: bool) -> Self {
        Self {
            plugin_id,
            prompt_visible_provenance,
            tools: HashMap::new(),
            descriptors: HashMap::new(),
        }
    }

    /// Add a tool from this plugin.
    pub fn add_tool(
        &mut self,
        tool: Arc<dyn Tool>,
        mut descriptor: ToolDescriptor,
    ) {
        descriptor.provenance = ToolProvenance::Plugin {
            id: self.plugin_id.clone(),
        };
        self.tools.insert(descriptor.name.clone(), tool);
        self.descriptors.insert(descriptor.name.clone(), descriptor);
    }

    /// Get the namespaced name for a tool.
    pub fn namespaced_name(&self, raw_name: &str) -> String {
        if self.prompt_visible_provenance {
            format!("plugin:{}:{}", self.plugin_id, raw_name)
        } else {
            raw_name.to_string()
        }
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

#[async_trait]
impl ToolProvider for PluginToolProvider {
    fn id(&self) -> &str {
        &self.plugin_id
    }

    async fn list_tools(&self) -> Vec<ToolDescriptor> {
        self.descriptors.values().cloned().collect()
    }

    async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let key =
            ToolName::parse(name).unwrap_or_else(|| ToolName::plain(name));
        self.tools.get(&key).cloned()
    }

    async fn on_tool_event(&self, _event: &ToolEvent) {}

    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}
