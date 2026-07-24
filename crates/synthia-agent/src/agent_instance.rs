use std::collections::HashMap;

use chrono::{DateTime, Utc};
use synthia_core::registry::RegistryItem;
use synthia_session::types::{Session, TokenBudget};
use tokio::sync::oneshot;

use crate::{
    control::fork_policy::ForkPolicy,
    registry::types::AgentDefinition,
};

/// Runtime status of an agent instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Running,
    Completed,
    Errored,
    Cancelled,
}

/// Result produced by a completed agent execution.
#[derive(Debug)]
pub struct AgentResult {
    pub output: String,
    pub status: AgentStatus,
    pub token_usage: AgentTokenUsage,
}

/// Token usage statistics for an agent execution.
#[derive(Debug)]
pub struct AgentTokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

/// Unified agent instance combining registry runtime state, coordinator
/// configuration, and execution bridge fields.
#[derive(Debug)]
pub struct AgentInstance {
    // From registry::instance::AgentInstance
    pub id: String,
    pub definition: Option<AgentDefinition>,
    pub session: Option<Session>,
    pub token_budget: Option<TokenBudget>,
    pub state: AgentStatus,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
    // From tools::agent_tools::coordinator::AgentInstance
    pub role: String,
    pub capabilities: Vec<String>,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    // Execution bridge
    pub fork_policy: ForkPolicy,
    pub depth: usize,
    pub result_tx: Option<oneshot::Sender<AgentResult>>,
}

impl RegistryItem for AgentInstance {
    fn name(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.role
    }
}

impl Clone for AgentInstance {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            definition: self.definition.clone(),
            session: self.session.clone(),
            token_budget: self.token_budget.clone(),
            state: self.state,
            parent_id: self.parent_id.clone(),
            created_at: self.created_at,
            role: self.role.clone(),
            capabilities: self.capabilities.clone(),
            system_prompt: self.system_prompt.clone(),
            tools: self.tools.clone(),
            metadata: self.metadata.clone(),
            fork_policy: self.fork_policy.clone(),
            depth: self.depth,
            result_tx: None,
        }
    }
}

impl AgentInstance {
    pub fn new(
        id: String,
        role: String,
        capabilities: Vec<String>,
        system_prompt: String,
        tools: Vec<String>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id,
            definition: None,
            session: None,
            token_budget: None,
            state: AgentStatus::Idle,
            parent_id: None,
            created_at: Utc::now(),
            role,
            capabilities,
            system_prompt,
            tools,
            metadata,
            fork_policy: ForkPolicy::SystemOnly,
            depth: 0,
            result_tx: None,
        }
    }

    /// Check if this agent has the required capability (case-insensitive matching).
    pub fn has_capability(&self, capability: &str) -> bool {
        let capability_lower = capability.to_lowercase();
        self.capabilities
            .iter()
            .any(|c| c.to_lowercase() == capability_lower)
    }
}
