//! Agent lifecycle tools: `Handoff`, `AgentStatus`, `RegisterAgent`.
//!
//! `Handoff` transfers a task to another agent via the message bus.
//! `AgentStatus` queries an agent's configuration (or lists all
//! registered agents). `RegisterAgent` dynamically registers a new
//! configurable agent with the coordinator.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::{
    builtin_types::is_builtin_subagent_type,
    bus::{AgentMessage, MessageBus},
    coordinator::{AgentCoordinator, AgentInstance},
};

/// Tool for transferring tasks between agents via message bus
pub struct HandoffTool {
    message_bus: Arc<dyn MessageBus>,
    sender_id: String,
}

impl HandoffTool {
    pub fn new(message_bus: Arc<dyn MessageBus>, sender_id: String) -> Self {
        Self {
            message_bus,
            sender_id,
        }
    }
}

#[async_trait]
impl Tool for HandoffTool {
    fn name(&self) -> &str {
        "Handoff"
    }

    fn description(&self) -> &str {
        "Hands off a task to another agent by sending a message through the message bus"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "target_agent_id": { "type": "string", "description": "The agent ID to handoff to" },
                "content": {
                    "type": "object",
                    "description": "The content to handoff (task, context, priority, etc.)"
                }
            },
            "required": ["target_agent_id", "content"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        // Extract required field: target_agent_id
        let target_agent_id =
            match input.input.get("target_agent_id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => {
                    return ToolOutput::error("target_agent_id is required");
                }
            };

        // Extract required field: content
        let content = match input.input.get("content") {
            Some(c) => c.to_string(),
            None => return ToolOutput::error("content is required"),
        };

        // Create agent message
        let message = AgentMessage::new(
            self.sender_id.clone(),
            target_agent_id.to_string(),
            content,
        );

        // Send message through the message bus
        match self.message_bus.send(message).await {
            Ok(()) => ToolOutput::text(format!(
                "Task handed off successfully to agent '{}'",
                target_agent_id
            )),
            Err(e) => ToolOutput::error(format!("Handoff failed: {}", e)),
        }
    }
}

/// Tool for querying the status and configuration of registered agents
pub struct AgentStatusTool {
    coordinator: Arc<AgentCoordinator>,
}

impl AgentStatusTool {
    pub fn new(coordinator: Arc<AgentCoordinator>) -> Self {
        Self { coordinator }
    }

    /// Format a single agent's status as a string
    fn format_agent_status(&self, agent: &AgentInstance) -> String {
        let mut result = String::new();
        result.push_str(&format!("ID: {}\n", agent.id));
        result.push_str(&format!("Role: {}\n", agent.role));
        result.push_str(&format!("Capabilities: {:?}\n", agent.capabilities));
        if !agent.tools.is_empty() {
            result.push_str(&format!("Tools: {:?}\n", agent.tools));
        }
        if !agent.metadata.is_empty() {
            result.push_str(&format!("Metadata: {:?}\n", agent.metadata));
        }
        result.push_str(&format!(
            "System Prompt: {} chars\n",
            agent.system_prompt.len()
        ));
        result
    }
}

#[async_trait]
impl Tool for AgentStatusTool {
    fn name(&self) -> &str {
        "AgentStatus"
    }

    fn description(&self) -> &str {
        "Queries the status and configuration of registered agents. Returns a single agent's status if agent_id is provided, or lists all registered agents if no agent_id is specified."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The agent ID to query. Optional - if not provided, returns status of all registered agents."
                }
            },
            "required": []
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        // Check if agent_id is provided
        match input.input.get("agent_id").and_then(|v| v.as_str()) {
            Some(agent_id) => {
                // Query single agent
                match self.coordinator.get_agent(agent_id) {
                    Ok(agent) => {
                        let status = self.format_agent_status(&agent);
                        ToolOutput::text(format!("Agent Status:\n{}", status))
                    }
                    Err(e) => {
                        ToolOutput::error(format!("Agent not found: {}", e))
                    }
                }
            }
            None => {
                // List all agents
                let agents = self.coordinator.list_agents();
                if agents.is_empty() {
                    ToolOutput::text("No agents registered.".to_string())
                } else {
                    let mut result = format!(
                        "Total registered agents: {}\n\n",
                        agents.len()
                    );
                    for (i, agent) in agents.iter().enumerate() {
                        result.push_str(&format!("--- Agent {} ---\n", i + 1));
                        result.push_str(&self.format_agent_status(agent));
                        result.push('\n');
                    }
                    ToolOutput::text(result)
                }
            }
        }
    }
}

/// Tool for dynamically registering configurable agents
pub struct RegisterAgentTool {
    coordinator: Arc<AgentCoordinator>,
}

impl RegisterAgentTool {
    pub fn new(coordinator: Arc<AgentCoordinator>) -> Self {
        Self { coordinator }
    }
}

#[async_trait]
impl Tool for RegisterAgentTool {
    fn name(&self) -> &str {
        "RegisterAgent"
    }

    fn description(&self) -> &str {
        "Dynamically registers a new configurable agent with the specified capabilities and metadata"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "Unique identifier for the agent" },
                "role": { "type": "string", "description": "Agent role (e.g., planner, executor, reviewer)" },
                "capabilities": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of agent capabilities"
                },
                "system_prompt": { "type": "string", "description": "System prompt for the agent" },
                "tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of tools the agent can use"
                },
                "metadata": { "type": "object", "description": "Additional metadata for the agent" }
            },
            "required": ["agent_id"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        // Extract required field: agent_id
        let agent_id =
            match input.input.get("agent_id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => return ToolOutput::error("agent_id is required"),
            };

        // Reject attempts to override reserved built-in subagent types.
        if is_builtin_subagent_type(agent_id) {
            return ToolOutput::error(format!(
                "'{}' is a reserved built-in subagent type",
                agent_id
            ));
        }

        // Extract optional fields
        let role = input
            .input
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("generic")
            .to_string();

        let capabilities: Vec<String> = input
            .input
            .get("capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let system_prompt = input
            .input
            .get("system_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tools: Vec<String> = input
            .input
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let metadata: HashMap<String, serde_json::Value> = input
            .input
            .get("metadata")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            })
            .unwrap_or_default();

        // Create AgentInstance
        let agent = AgentInstance::new(
            agent_id.to_string(),
            role.clone(),
            capabilities.clone(),
            system_prompt,
            tools.clone(),
            metadata,
        );

        // Register the agent
        match self.coordinator.register_agent(agent) {
            Ok(()) => {
                let mut result =
                    format!("Agent '{}' registered successfully\n", agent_id);
                result.push_str(&format!("Role: {}\n", role));
                result.push_str(&format!("Capabilities: {:?}\n", capabilities));
                if !tools.is_empty() {
                    result.push_str(&format!("Tools: {:?}\n", tools));
                }
                ToolOutput::text(result)
            }
            Err(e) => ToolOutput::error(e.to_string()),
        }
    }
}
