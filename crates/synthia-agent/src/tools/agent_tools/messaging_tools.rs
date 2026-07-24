//! Inter-agent messaging tools: `SendMessage`, `TeamCreate`, `TeamDelete`.
//!
//! These tools let a parent agent talk to a child agent, create a named
//! team of agents, and disband a team. All operations go through
//! [`SubagentManager`] — there is no direct manipulation of the
//! underlying [`AgentCoordinator`] or [`InMemoryMessageBus`] from the
//! tool layer.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::team::SubagentManager;

pub struct SendMessageTool {
    manager: Arc<SubagentManager>,
}

impl SendMessageTool {
    pub fn new(manager: Arc<SubagentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "SendMessage"
    }

    fn description(&self) -> &str {
        "Sends a message to an agent teammate, or resumes a subagent by its agent ID"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "The agent ID to send message to" },
                "message": { "type": "string", "description": "The message to send" }
            },
            "required": ["agent_id", "message"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let agent_id =
            match input.input.get("agent_id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => return ToolOutput::error("agent_id is required"),
            };
        let message = match input.input.get("message").and_then(|v| v.as_str())
        {
            Some(m) => m,
            None => return ToolOutput::error("message is required"),
        };
        if self.manager.send_message(agent_id, message) {
            ToolOutput::text(format!("Message sent to agent {}", agent_id))
        } else {
            ToolOutput::text(format!("Agent '{}' not found", agent_id))
        }
    }
}

pub struct TeamCreateTool {
    manager: Arc<SubagentManager>,
}

impl TeamCreateTool {
    pub fn new(manager: Arc<SubagentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &str {
        "TeamCreate"
    }

    fn description(&self) -> &str {
        "Creates an agent team with multiple teammates"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The team name" },
                "members": { "type": "array", "items": { "type": "string" }, "description": "List of member names" }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let name = match input.input.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolOutput::error("name is required"),
        };
        let members: Vec<String> = input
            .input
            .get("members")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let team = self.manager.create_team(name, members);
        ToolOutput::text(format!(
            "Team created: {} (ID: {}, members: {:?})",
            team.name, team.id, team.members
        ))
    }
}

pub struct TeamDeleteTool {
    manager: Arc<SubagentManager>,
}

impl TeamDeleteTool {
    pub fn new(manager: Arc<SubagentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TeamDeleteTool {
    fn name(&self) -> &str {
        "TeamDelete"
    }

    fn description(&self) -> &str {
        "Disbands an agent team and cleans up teammate processes"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "team_id": { "type": "string", "description": "The team ID to disband" }
            },
            "required": ["team_id"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let team_id = match input.input.get("team_id").and_then(|v| v.as_str())
        {
            Some(id) => id,
            None => return ToolOutput::error("team_id is required"),
        };
        if self.manager.delete_team(team_id) {
            ToolOutput::text(format!("Team '{}' disbanded", team_id))
        } else {
            ToolOutput::text(format!("Team '{}' not found", team_id))
        }
    }
}
