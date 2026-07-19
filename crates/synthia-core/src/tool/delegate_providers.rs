//! Delegate providers — SkillToolProvider, SubagentToolProvider, DynamicToolProvider.
//!
//! These wrap existing registries and delegate to them.

use std::sync::Arc;

use async_trait::async_trait;

use crate::tool::{
    Tool,
    descriptor::ToolDescriptor,
    provider::{ToolCall, ToolEvent, ToolProvider},
    types::{ToolError, ToolOutput},
};

// ─── SkillToolProvider ────────────────────────────────────────────

/// Tool provider backed by the skill registry.
pub struct SkillToolProvider {
    provider_id: String,
}

impl Default for SkillToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillToolProvider {
    pub fn new() -> Self {
        Self {
            provider_id: "skill".to_string(),
        }
    }
}

#[async_trait]
impl ToolProvider for SkillToolProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    async fn list_tools(&self) -> Vec<ToolDescriptor> {
        // TODO: delegate to SkillRegistry once service layer is wired
        Vec::new()
    }

    async fn get_tool(&self, _name: &str) -> Option<Arc<dyn Tool>> {
        // TODO: delegate to SkillRegistry
        None
    }

    async fn on_tool_event(&self, _event: &ToolEvent) {}

    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}

// ─── SubagentToolProvider ─────────────────────────────────────────

/// Tool provider backed by the subagent session factory.
pub struct SubagentToolProvider {
    provider_id: String,
}

impl Default for SubagentToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SubagentToolProvider {
    pub fn new() -> Self {
        Self {
            provider_id: "subagent".to_string(),
        }
    }
}

#[async_trait]
impl ToolProvider for SubagentToolProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    async fn list_tools(&self) -> Vec<ToolDescriptor> {
        // TODO: delegate to SubagentSessionFactory once service layer is wired
        Vec::new()
    }

    async fn get_tool(&self, _name: &str) -> Option<Arc<dyn Tool>> {
        // TODO: delegate to SubagentSessionFactory
        None
    }

    async fn on_tool_event(&self, _event: &ToolEvent) {}

    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}

// ─── DynamicToolProvider ──────────────────────────────────────────

/// Tool provider for script-based / dynamically registered tools.
pub struct DynamicToolProvider {
    provider_id: String,
}

impl Default for DynamicToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicToolProvider {
    pub fn new() -> Self {
        Self {
            provider_id: "dynamic".to_string(),
        }
    }
}

#[async_trait]
impl ToolProvider for DynamicToolProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    async fn list_tools(&self) -> Vec<ToolDescriptor> {
        // TODO: list dynamically registered tools
        Vec::new()
    }

    async fn get_tool(&self, _name: &str) -> Option<Arc<dyn Tool>> {
        // TODO: resolve dynamic tool
        None
    }

    async fn on_tool_event(&self, _event: &ToolEvent) {}

    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}
