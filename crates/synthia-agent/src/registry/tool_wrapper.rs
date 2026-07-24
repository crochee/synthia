use std::sync::Arc;

use async_trait::async_trait;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::agent_registry::AgentRegistry;

pub struct AgentToolWrapper {
    pub instance_id: String,
    pub definition: super::types::AgentDefinition,
    pub agent_registry: Arc<AgentRegistry>,
}

impl AgentToolWrapper {
    pub fn new(
        instance_id: String,
        definition: super::types::AgentDefinition,
        agent_registry: Arc<AgentRegistry>,
    ) -> Self {
        Self {
            instance_id,
            definition,
            agent_registry,
        }
    }
}

#[async_trait]
impl Tool for AgentToolWrapper {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["task"],
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The task to assign to this agent"
                }
            }
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let task_description = input
            .input
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if task_description.is_empty() {
            return ToolOutput::error(
                "Task parameter is required and cannot be empty",
            );
        }

        ToolOutput::text(format!(
            "Agent '{}' received task: {}. Task scheduling is handled by the LLM.",
            self.definition.name, task_description
        ))
    }
}

#[cfg(test)]
mod tests {
    use synthia_core::Registry;

    use super::*;
    use crate::registry::AgentRegistry;

    #[tokio::test]
    async fn test_tool_wrapper_returns_task_info() {
        let registry = AgentRegistry::new();

        let def = super::super::types::AgentDefinition {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            capabilities: vec!["test".to_string()],
            when_to_use: vec![],
            constraints: vec![],
            system_prompt: "Test prompt".to_string(),
            source_path: std::path::PathBuf::from("/tmp"),
            file_hash: "abc123".to_string(),
            loaded_at: chrono::Utc::now(),
            enabled: true,
            permission_rules: vec![],
            permission_default: None,
            tools: None,
            denied_tools: None,
            extends: None,
            mode: None,
        };
        registry.register(def.clone()).await.unwrap();

        let instance_id = registry.spawn("test-agent", None, None).unwrap();
        let wrapper = registry.wrap_as_tool(&instance_id).unwrap();

        let input = synthia_tool::types::ToolInput {
            name: "Test Agent".to_string(),
            input: serde_json::json!({ "task": "Review code changes" }),
            context: synthia_tool::types::ToolExecutionContext::new(
                "test-session".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        };

        let result = wrapper.call(input).await;
        assert!(result.is_text());
    }

    #[tokio::test]
    async fn test_tool_wrapper_requires_task() {
        let registry = AgentRegistry::new();

        let def = super::super::types::AgentDefinition {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            capabilities: vec!["test".to_string()],
            when_to_use: vec![],
            constraints: vec![],
            system_prompt: "Test prompt".to_string(),
            source_path: std::path::PathBuf::from("/tmp"),
            file_hash: "abc123".to_string(),
            loaded_at: chrono::Utc::now(),
            enabled: true,
            permission_rules: vec![],
            permission_default: None,
            tools: None,
            denied_tools: None,
            extends: None,
            mode: None,
        };
        registry.register(def).await.unwrap();

        let instance_id = registry.spawn("test-agent", None, None).unwrap();
        let wrapper = registry.wrap_as_tool(&instance_id).unwrap();

        let input = synthia_tool::types::ToolInput {
            name: "Test Agent".to_string(),
            input: serde_json::json!({}),
            context: synthia_tool::types::ToolExecutionContext::new(
                "test-session".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        };

        let result = wrapper.call(input).await;
        assert!(result.is_error.unwrap_or(true));
    }
}
