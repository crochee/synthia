//! Subagent tool implementation

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;
use tracing::info;

use super::{executor::SubagentExecutor, types::SubagentRequest};
use crate::{config::AgentConfig, tools::Tool};

#[derive(Clone)]
pub struct SubagentTool {
    configs: HashMap<String, AgentConfig>,
    description: String,
    executor: Arc<SubagentExecutor>,
}

impl std::fmt::Debug for SubagentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubagentTool")
            .field("configs", &self.configs.keys().collect::<Vec<_>>())
            .field("description", &self.description)
            .finish()
    }
}

impl SubagentTool {
    pub fn new(executor: Arc<SubagentExecutor>) -> Self {
        Self {
            configs: HashMap::new(),
            description: "Launch an isolated subagent for focused tasks."
                .to_string(),
            executor,
        }
    }

    pub fn with_configs(mut self, configs: Vec<AgentConfig>) -> Self {
        self.configs =
            configs.into_iter().map(|c| (c.name.clone(), c)).collect();
        self.description = Self::build_description(&self.configs);
        self
    }

    fn build_description(configs: &HashMap<String, AgentConfig>) -> String {
        if configs.is_empty() {
            return "Launch an isolated subagent for focused tasks."
                .to_string();
        }

        let items: Vec<String> = configs
            .iter()
            .map(|(name, config)| format!("- {}: {}", name, config.description))
            .collect();

        format!(
            "Launch an isolated subagent for focused tasks. Subagents run independently.\nAvailable subagents:\n{}\n",
            items.join("\n")
        )
    }
}

#[async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "subagent": {
                    "type": "string",
                    "description": "The subagent name to use (see tool description for available subagents)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task instructions for the subagent"
                },
                "context": {
                    "type": "string",
                    "description": "Optional context to prepend (e.g. relevant code, prior findings)"
                }
            },
            "required": ["subagent", "prompt"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: SubagentRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let config = match self.configs.get(&request.subagent) {
            Some(c) => c,
            None => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Unknown subagent: {}",
                    request.subagent
                ))]);
            }
        };

        let guard = match self.executor.guards().reserve(&request.subagent) {
            Ok(g) => g,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to reserve guard: {e}"
                ))]);
            }
        };

        info!(
            subagent = %request.subagent,
            prompt_len = request.prompt.len(),
            thread_id = %guard.thread_id(),
            active_threads = self.executor.guards().active_thread_count(),
            max_threads = self.executor.guards().max_threads(),
            "Starting subagent"
        );

        match self
            .executor
            .execute(config, request.prompt, request.context)
            .await
        {
            Ok(response) => {
                CallToolResult::success(vec![Content::text(response)])
            }
            Err(e) => {
                tracing::error!(
                    subagent = %request.subagent,
                    thread_id = %guard.thread_id(),
                    error = %e,
                    "Subagent execution failed"
                );
                CallToolResult::error(vec![Content::text(format!(
                    "Subagent execution failed: {e}"
                ))])
            }
        }
    }
}
