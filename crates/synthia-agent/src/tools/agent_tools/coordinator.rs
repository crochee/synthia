//! Agent registry + task coordination.
//!
//! [`AgentInstance`] is a registered agent's static configuration (id, role,
//! capabilities, system prompt, tool list, free-form metadata). [`AgentCoordinator`]
//! maintains the registry, performs capability-based task assignment, and
//! tracks task dependencies and results.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use dashmap::DashMap;
use parking_lot::Mutex;
use synthia_task::types::StructuredOutput;
use thiserror::Error;

use super::bus::{InMemoryMessageBus, MessageBus};
use crate::task::types::TaskResult;

/// Error type for agent operations
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Agent '{0}' already registered")]
    AlreadyRegistered(String),
    #[error("Agent '{0}' not found")]
    NotFound(String),
    #[error("Task '{0}' already assigned")]
    TaskAlreadyAssigned(String),
    #[error("Task '{0}' not found")]
    TaskNotFound(String),
    #[error("Capability mismatch: {0}")]
    CapabilityMismatch(String),
}

pub use crate::agent_instance::AgentInstance;

/// Coordinator for managing multiple agents and task distribution
pub struct AgentCoordinator {
    message_bus: Arc<InMemoryMessageBus>,
    agents: Arc<DashMap<String, AgentInstance>>,
    task_dependencies: Arc<DashMap<String, HashSet<String>>>,
    completed_tasks: Arc<Mutex<HashSet<String>>>,
    task_results: Arc<DashMap<String, TaskResult>>,
}

impl AgentCoordinator {
    pub fn new(message_bus: Arc<InMemoryMessageBus>) -> Self {
        Self {
            message_bus,
            agents: Arc::new(DashMap::new()),
            task_dependencies: Arc::new(DashMap::new()),
            completed_tasks: Arc::new(Mutex::new(HashSet::new())),
            task_results: Arc::new(DashMap::new()),
        }
    }

    /// Register a new agent
    pub fn register_agent(
        &self,
        agent: AgentInstance,
    ) -> Result<(), AgentError> {
        let agent_id = &agent.id;
        if self.agents.contains_key(agent_id) {
            return Err(AgentError::AlreadyRegistered(agent_id.clone()));
        }

        self.message_bus
            .register_agent(agent_id)
            .map_err(|e| AgentError::CapabilityMismatch(e.to_string()))?;

        self.agents.insert(agent_id.clone(), agent);
        Ok(())
    }

    /// Get an agent by ID
    pub fn get_agent(
        &self,
        agent_id: &str,
    ) -> Result<AgentInstance, AgentError> {
        self.agents
            .get(agent_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| AgentError::NotFound(agent_id.to_string()))
    }

    /// List all registered agents
    pub fn list_agents(&self) -> Vec<AgentInstance> {
        self.agents.iter().map(|r| r.value().clone()).collect()
    }

    /// Assign a task to an agent based on capability matching
    pub fn assign_task(
        &self,
        task_id: &str,
        required_capability: &str,
    ) -> Result<String, AgentError> {
        if self.task_results.contains_key(task_id) {
            return Err(AgentError::TaskAlreadyAssigned(task_id.to_string()));
        }

        let capability_lower = required_capability.to_lowercase();
        let agent_id = self
            .agents
            .iter()
            .find(|r| {
                r.capabilities
                    .iter()
                    .any(|c| c.to_lowercase() == capability_lower)
            })
            .map(|r| r.key().clone())
            .ok_or_else(|| {
                AgentError::CapabilityMismatch(format!(
                    "No agent found with capability '{}'",
                    required_capability
                ))
            })?;

        Ok(agent_id)
    }

    /// Add a dependency for a task (task_id depends on dependency_id)
    pub fn add_dependency(&self, task_id: String, dependency_id: String) {
        self.task_dependencies
            .entry(task_id)
            .or_default()
            .insert(dependency_id);
    }

    /// Check if a task can be scheduled (all dependencies are completed)
    pub fn can_schedule(&self, task_id: &str) -> bool {
        if let Some(deps) = self.task_dependencies.get(task_id) {
            let completed = self.completed_tasks.lock();
            deps.iter().all(|dep| completed.contains(dep))
        } else {
            true
        }
    }

    /// Get all tasks that are ready to be scheduled (dependencies satisfied)
    pub fn get_ready_tasks(&self) -> Vec<String> {
        self.task_dependencies
            .iter()
            .filter(|entry| {
                let deps = entry.value();
                let completed = self.completed_tasks.lock();
                deps.iter().all(|dep| completed.contains(dep))
            })
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Store the result of a completed task
    pub fn store_result(&self, task_id: String, result: TaskResult) {
        self.completed_tasks.lock().insert(task_id.clone());
        self.task_results.insert(task_id, result);
    }

    /// Collect all results for given task IDs
    pub fn collect_results(
        &self,
        task_ids: &[String],
    ) -> HashMap<String, TaskResult> {
        task_ids
            .iter()
            .filter_map(|id| {
                self.task_results.get(id).map(|r| (id.clone(), r.clone()))
            })
            .collect()
    }

    /// Aggregate outputs from multiple tasks into structured outputs
    pub fn aggregate_outputs(
        &self,
        task_ids: &[String],
    ) -> Vec<StructuredOutput> {
        task_ids
            .iter()
            .filter_map(|id| {
                self.task_results.get(id).map(|result| StructuredOutput {
                    key: id.clone(),
                    value: serde_json::json!({
                        "output": result.output,
                        "status": format!("{:?}", result.status),
                        "exit_code": result.exit_code,
                        "artifacts": result.artifacts,
                    }),
                })
            })
            .collect()
    }
}
