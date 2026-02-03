//! Subagent executor implementation

use std::sync::Arc;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{info, info_span};

use super::types::{DENIED_TOOLS, ExecutorConfig, SubagentContextOverrides};
use crate::{
    Agent,
    AgentError,
    AgentEventHandler,
    agent::{AgentControl, AgentDeps, Guards},
    config::{AgentConfig, SessionConfig},
    context::ContextManager,
    guardian::{Guardian, GuardianConfig, SimpleGuardian},
    hooks::HookRegistry,
    model_router::ModelRouter,
    session::SessionManager,
    tools::{SkillTool, ToolRegistry},
    types::AgentEvent,
    utils::extract_response_text,
};

#[derive(Clone)]
pub struct SubagentExecutor {
    tool_registry: Arc<ToolRegistry>,
    context_manager: Arc<dyn ContextManager>,
    session_manager: Arc<dyn SessionManager>,
    model_router: Arc<dyn ModelRouter>,
    hook_registry: Arc<HookRegistry>,
    skill_tool: Arc<SkillTool>,
    event_handler: Arc<dyn AgentEventHandler>,
    guards: Arc<Guards>,
}

/// Context fork for subagent execution
pub struct SubagentContext {
    pub session: crate::session::Session,
    pub should_avoid_permission_prompts: bool,
    pub content_replacement_state: std::collections::HashMap<String, String>,
}

impl SubagentExecutor {
    /// Create a forked context for subagent execution with optional overrides
    pub fn fork_context(
        &self,
        _overrides: SubagentContextOverrides,
    ) -> SubagentContext {
        SubagentContext {
            session: crate::session::Session::default(),
            should_avoid_permission_prompts: true,
            content_replacement_state: std::collections::HashMap::new(),
        }
    }

    pub fn new(config: ExecutorConfig) -> Self {
        Self {
            tool_registry: config.tool_registry,
            context_manager: config.context_manager,
            session_manager: config.session_manager,
            model_router: config.model_router,
            hook_registry: config.hook_registry,
            skill_tool: config.skill_tool,
            event_handler: config.event_handler,
            guards: config.guards,
        }
    }

    pub fn guards(&self) -> &Arc<Guards> {
        &self.guards
    }

    async fn create_agent(&self, config: &AgentConfig) -> Agent {
        let mut subagent_config = config.clone();
        for tool in DENIED_TOOLS {
            if !subagent_config.denied_tools.contains(&tool.to_string()) {
                subagent_config.denied_tools.push(tool.to_string());
            }
        }
        subagent_config.is_subagent = true;

        let deps = AgentDeps {
            tools: Arc::clone(&self.tool_registry),
            context: Arc::clone(&self.context_manager),
            session: Arc::clone(&self.session_manager),
            router: Arc::clone(&self.model_router),
            hooks: Arc::clone(&self.hook_registry),
            skills: Arc::clone(&self.skill_tool),
            guardian: Arc::new(SimpleGuardian::new(GuardianConfig::default()))
                as Arc<dyn Guardian>,
            control: Arc::new(AgentControl::new()),
        };

        Agent::new(Arc::new(subagent_config), deps)
    }

    pub async fn execute(
        &self,
        config: &AgentConfig,
        prompt: String,
        context: Option<String>,
    ) -> Result<String, AgentError> {
        let _span = info_span!("subagent_execution", subagent = %config.name);

        let subagent = self.create_agent(config).await;
        info!(subagent = %config.name, "Starting subagent execution");

        let session = self.session_manager.create_session().await?;
        let session_config = SessionConfig::from(session);
        let session_id = session_config.id.clone();

        let full_prompt = context
            .map(|ctx| format!("Context:\n{ctx}\n\n{prompt}"))
            .unwrap_or(prompt);

        let message = rmcp::model::SamplingMessage::user_text(&full_prompt);
        let event_handler = Arc::clone(&self.event_handler);

        let mut stream = subagent
            .reply(message, &session_config, CancellationToken::new())
            .await?;

        let mut messages = Vec::new();

        while let Some(event_result) = stream.next().await {
            let event = event_result?;
            event_handler.on_event(&config.name, &event).await;

            if let AgentEvent::Message(msg) = event {
                messages.push(msg);
            }
        }

        let result = extract_response_text(&messages);

        info!(
            session_id = %session_id,
            subagent = %config.name,
            "Subagent execution completed"
        );

        Ok(result)
    }
}
