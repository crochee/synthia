//! Subagent tool implementation
//!
//! This tool is only available in Solo mode. In Team mode, use the team
//! collaboration tools instead.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;
use tracing::info;

use super::{executor::SubagentExecutor, types::SubagentRequest};
use crate::{
    config::{AgentConfig, AgentName},
    tools::{Tool, subagent::types::SubagentRequestLegacy},
};

#[derive(Clone)]
pub struct SubagentTool {
    configs: HashMap<String, AgentConfig>,
    description: String,
    executor: Arc<SubagentExecutor>,
    default_config: Option<AgentConfig>,
    parent_name: AgentName,
}

impl std::fmt::Debug for SubagentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubagentTool")
            .field("configs", &self.configs.keys().collect::<Vec<_>>())
            .field("description", &self.description)
            .field("has_default_config", &self.default_config.is_some())
            .field("parent_name", &self.parent_name)
            .finish()
    }
}

impl SubagentTool {
    pub fn new(executor: Arc<SubagentExecutor>) -> Self {
        Self {
            configs: HashMap::new(),
            description: "Launch an isolated subagent for focused tasks (Solo mode only)."
                .to_string(),
            executor,
            default_config: None,
            parent_name: AgentName::Solo,
        }
    }

    pub fn with_configs(mut self, configs: Vec<AgentConfig>) -> Self {
        self.configs = configs
            .into_iter()
            .map(|c| (c.name.as_str().to_string(), c))
            .collect();
        self.description = Self::build_description(&self.configs);
        self
    }

    pub fn with_default_config(mut self, config: AgentConfig) -> Self {
        self.default_config = Some(config);
        self
    }

    pub fn with_parent_name(mut self, name: AgentName) -> Self {
        self.parent_name = name;
        self
    }

    fn build_description(configs: &HashMap<String, AgentConfig>) -> String {
        if configs.is_empty() {
            return "Launch an isolated subagent for focused tasks (Solo mode only)."
                .to_string();
        }

        let items: Vec<String> = configs
            .iter()
            .map(|(name, config)| format!("- {}: {}", name, config.description))
            .collect();

        format!(
            "Launch an isolated subagent for focused tasks (Solo mode only). Subagents run independently.\nAvailable subagents:\n{}\n",
            items.join("\n")
        )
    }

    fn check_mode(&self) -> Result<(), String> {
        if !self.parent_name.is_solo() {
            return Err("This tool is only available in Solo mode.".to_string());
        }
        Ok(())
    }

    fn resolve_config(
        &self,
        request: &SubagentRequest,
    ) -> Result<AgentConfig, String> {
        if let Some(ref name) = request.name
            && let Some(config) = self.configs.get(name)
        {
            return Ok(config.clone());
        }

        if let Some(ref subagent_type) = request.subagent_type {
            if let Some(config) = self.configs.get(subagent_type) {
                return Ok(config.clone());
            }
            for (name, config) in &self.configs {
                if name.contains(subagent_type)
                    || config.description.contains(subagent_type)
                {
                    return Ok(config.clone());
                }
            }
        }

        if let Some(ref default) = self.default_config {
            let mut config = default.clone();
            if let Some(ref name) = request.name {
                config.name = AgentName::Custom(name.clone());
            }
            if let Some(ref subagent_type) = request.subagent_type {
                config.description =
                    format!("{} (type: {})", config.description, subagent_type);
            }
            return Ok(config);
        }

        Err("No matching agent config found and no default configured"
            .to_string())
    }
}

#[async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "Agent"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "Human-readable description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task instructions for the subagent"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "Optional subagent type/class to use (e.g. 'code-reviewer', 'researcher')"
                },
                "name": {
                    "type": "string",
                    "description": "Optional name for the subagent instance"
                },
                "model": {
                    "type": "string",
                    "description": "Optional model to use for this subagent"
                },
                "context": {
                    "type": "string",
                    "description": "Optional context to prepend (e.g. relevant code, prior findings)"
                }
            },
            "required": ["description", "prompt"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        if let Err(e) = self.check_mode() {
            return CallToolResult::error(vec![Content::text(e)]);
        }

        let request: SubagentRequest =
            match serde_json::from_value(args.clone()) {
                Ok(r) => r,
                Err(_) => {
                    let legacy: SubagentRequestLegacy =
                        match serde_json::from_value(args) {
                            Ok(r) => r,
                            Err(e) => {
                                return CallToolResult::error(vec![
                                    Content::text(format!(
                                        "Invalid request format: {e}"
                                    )),
                                ]);
                            }
                        };
                    SubagentRequest {
                        description: format!(
                            "Task for subagent: {}",
                            legacy.subagent
                        ),
                        prompt: legacy.prompt,
                        subagent_type: Some(legacy.subagent),
                        name: None,
                        model: None,
                        context: legacy.context,
                    }
                }
            };

        let config = match self.resolve_config(&request) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to resolve agent config: {e}"
                ))]);
            }
        };

        let agent_name = config.name.clone();
        let guard = match self.executor.guards().reserve(agent_name.as_str()) {
            Ok(g) => g,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to reserve guard: {e}"
                ))]);
            }
        };

        info!(
            subagent = %agent_name,
            subagent_type = ?request.subagent_type,
            prompt_len = request.prompt.len(),
            thread_id = %guard.thread_id(),
            active_threads = self.executor.guards().active_thread_count(),
            max_threads = self.executor.guards().max_threads(),
            "Starting subagent"
        );

        match self
            .executor
            .execute(&config, request.prompt, request.context)
            .await
        {
            Ok(response) => {
                CallToolResult::success(vec![Content::text(response)])
            }
            Err(e) => {
                tracing::error!(
                    subagent = %agent_name,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_name_solo() {
        let name = AgentName::Solo;
        assert!(name.is_solo());
    }

    #[test]
    fn test_check_name_lead() {
        let name = AgentName::Lead;
        assert!(name.is_lead());
    }

    #[test]
    fn test_check_name_custom() {
        let name = AgentName::Custom("member".to_string());
        assert!(name.is_custom());
    }

    #[test]
    fn test_build_description_includes_solo_mode() {
        let configs: HashMap<String, AgentConfig> = HashMap::new();
        let description = SubagentTool::build_description(&configs);
        assert!(description.contains("Solo mode"));
    }

    #[test]
    fn test_build_description_with_configs_includes_solo_mode() {
        let mut configs: HashMap<String, AgentConfig> = HashMap::new();
        let config = AgentConfig::default();
        configs.insert("test".to_string(), config);
        let description = SubagentTool::build_description(&configs);
        assert!(description.contains("Solo mode"));
    }
}
