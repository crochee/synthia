use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    AgentEventHandler,
    agent::Guards,
    context::ContextManager,
    hooks::HookRegistry,
    model_router::ModelRouter,
    session::SessionManager,
    tools::{SkillTool, ToolRegistry},
};

/// Dynamic subagent request matching TOOL_SPEC.md Agent tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRequest {
    /// Human-readable description of the task
    pub description: String,
    /// Task instructions for the subagent
    pub prompt: String,
    /// Optional subagent type/class to use
    #[serde(default)]
    pub subagent_type: Option<String>,
    /// Optional name for the subagent instance
    #[serde(default)]
    pub name: Option<String>,
    /// Optional model to use for this subagent
    #[serde(default)]
    pub model: Option<String>,
    /// Optional context to prepend
    #[serde(default)]
    pub context: Option<String>,
}

/// Legacy request format for backward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SubagentRequestLegacy {
    pub subagent: String,
    pub prompt: String,
    #[serde(default)]
    pub context: Option<String>,
}

pub(crate) const DENIED_TOOLS: &[&str] = &["askUserQuestion", "spawn_agent"];

/// Overrides for subagent context forking
#[derive(Debug, Clone, Default)]
pub struct SubagentContextOverrides {
    pub session: Option<crate::session::Session>,
    pub should_avoid_permission_prompts: Option<bool>,
}

pub struct ExecutorConfig {
    pub tool_registry: Arc<ToolRegistry>,
    pub context_manager: Arc<dyn ContextManager>,
    pub session_manager: Arc<dyn SessionManager>,
    pub model_router: Arc<dyn ModelRouter>,
    pub hook_registry: Arc<HookRegistry>,
    pub skill_tool: Arc<SkillTool>,
    pub event_handler: Arc<dyn AgentEventHandler>,
    pub guards: Arc<Guards>,
}
