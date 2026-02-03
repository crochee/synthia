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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRequest {
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
