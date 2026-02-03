//! Agent building and subagent creation

use std::{path::Path, sync::Arc};

use anyhow::Result;
use synthia_agent::{
    Agent,
    agent::{AgentControl, Guards, builtins},
    config::AgentConfig,
    context::DefaultContextManager,
    hooks::{HookRegistry, LoggingHook},
    model_router::FirstModelRouter,
    session::{SessionFileStore, SessionManager},
    shell::LocalShellExecutor,
    tools::{
        AskUserQuestionTool,
        ExecTool,
        QuestionSenderImpl,
        SkillTool,
        SubagentExecutor,
        SubagentTool,
        ToolRegistry,
        register_background_tools,
        register_builtin_tools,
        register_task_tools,
        register_team_tools,
        register_worktree_tools,
    },
};
use synthia_job::TimeWheel;

use crate::{
    config::ServerConfig,
    mcp::McpService,
    state::{AppState, ServerEventHandler},
};

pub fn create_subagent_tool(
    registry: &Arc<ToolRegistry>,
    context_manager: Arc<dyn synthia_agent::context::ContextManager>,
    session_manager: Arc<dyn SessionManager>,
    model_router: Arc<dyn synthia_agent::model_router::ModelRouter>,
    hook_registry: Arc<HookRegistry>,
    skill_tool: Arc<SkillTool>,
    guards: Arc<Guards>,
) -> SubagentTool {
    let event_handler = Arc::new(ServerEventHandler);
    let subagent_executor =
        SubagentExecutor::new(synthia_agent::tools::ExecutorConfig {
            tool_registry: registry.clone(),
            context_manager,
            session_manager,
            model_router,
            hook_registry,
            skill_tool,
            event_handler,
            guards,
        });
    SubagentTool::new(Arc::new(subagent_executor))
}

pub async fn register_tools(
    registry: &Arc<ToolRegistry>,
    question_sender: &Arc<QuestionSenderImpl>,
    _time_wheel: Arc<TimeWheel>,
    _agent: Agent,
    current_dir: &Path,
    executor: Arc<LocalShellExecutor>,
) -> Result<()> {
    register_builtin_tools(registry).await;
    registry
        .register(Arc::new(AskUserQuestionTool::new(question_sender.clone())))
        .await;
    register_task_tools(registry).await;
    register_team_tools(registry).await;
    register_worktree_tools(registry, current_dir.to_path_buf()).await;
    registry
        .register(Arc::new(ExecTool::new(executor.clone())))
        .await;
    register_background_tools(registry, executor.clone()).await;
    Ok(())
}

pub async fn build_agent(
    current_dir: &std::path::PathBuf,
    config: &ServerConfig,
) -> Result<AppState> {
    let question_sender = Arc::new(QuestionSenderImpl::new());
    let time_wheel = Arc::new(TimeWheel::new());
    let tool_registry = Arc::new(ToolRegistry::new());
    let executor = Arc::new(LocalShellExecutor::new());

    let agent_config = AgentConfig::default();
    let hook_registry = Arc::new(HookRegistry::new());
    hook_registry
        .register(Arc::new(LoggingHook::new()) as synthia_agent::hooks::HookPtr)
        .await;

    let skill_tool = Arc::new(SkillTool::new(current_dir.clone()));
    let model_router: Arc<dyn synthia_agent::model_router::ModelRouter> =
        Arc::new(FirstModelRouter::default());
    let session_manager: Arc<dyn SessionManager> =
        Arc::new(SessionFileStore::new());
    let context_manager =
        Arc::new(DefaultContextManager::new(model_router.clone()));

    let agent = Agent::new(
        Arc::new(agent_config),
        synthia_agent::agent::AgentDeps {
            tools: tool_registry.clone(),
            context: context_manager.clone(),
            session: session_manager.clone(),
            router: model_router.clone(),
            hooks: hook_registry.clone(),
            skills: skill_tool.clone(),
            guardian: Arc::new(synthia_agent::guardian::AdvancedGuardian::new(
                synthia_agent::guardian::GuardianConfig::default(),
                model_router.clone(),
                context_manager.clone(),
            ))
                as Arc<dyn synthia_agent::guardian::Guardian>,
            control: Arc::new(AgentControl::new()),
        },
    );

    register_tools(
        &tool_registry,
        &question_sender,
        time_wheel.clone(),
        agent.clone(),
        current_dir,
        executor.clone(),
    )
    .await?;

    let max_threads = config.max_agents;
    let guards = Arc::new(Guards::new(Some(max_threads as u32)));
    let mut all_agents = config.get_agents(current_dir);
    let built_in_agents = builtins::built_in::configs(current_dir);
    for (name, agent_config) in built_in_agents {
        if !all_agents.iter().any(|a| a.name == *name) {
            all_agents.push(agent_config.clone());
        }
    }

    let subagent_tool = create_subagent_tool(
        &tool_registry,
        agent.deps.context.clone(),
        agent.deps.session.clone(),
        agent.deps.router.clone(),
        agent.deps.hooks.clone(),
        skill_tool,
        guards,
    );
    tool_registry.register(Arc::new(subagent_tool)).await;

    let mcp_module = McpService::new();

    for (name, mcp_config) in &config.mcps {
        let mut server_config: crate::mcp::McpServerConfig =
            mcp_config.clone().into();
        server_config.name = name.clone();
        if let Err(e) = mcp_module.register_server(server_config).await {
            tracing::warn!("Failed to register MCP server '{}': {}", name, e);
        }
    }

    if let Err(e) = mcp_module.start_all().await {
        tracing::warn!("Failed to start some MCP servers: {}", e);
    }

    let server_config = Arc::new(tokio::sync::RwLock::new(config.clone()));

    Ok(AppState {
        agent,
        session_manager,
        tool_registry,
        current_dir: current_dir.clone(),
        mcp_module,
        config: server_config,
        config_path: current_dir.clone(),
        config_host: config.host.clone(),
        config_port: config.port,
    })
}
