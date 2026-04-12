use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use synthia_agent::{
    Agent,
    agent::{AgentControl, Guards, builtins},
    config::AgentConfig,
    context::DefaultContextManager,
    guardian::{AdvancedGuardian, Guardian, GuardianConfig},
    hooks::{HookRegistry, LoggingHook},
    model_router::FirstModelRouter,
    session::{SessionFileStore, SessionManager},
    shell::LocalShellExecutor,
    tools::{
        AskUserQuestionTool,
        CronFileStore,
        ExecTool,
        ExecutorConfig,
        QuestionSenderImpl,
        SkillTool,
        SubagentExecutor,
        SubagentTool,
        ToolRegistry,
        get_mcp_tools,
        register_background_tools,
        register_builtin_tools,
        register_cron_tools,
        register_task_tools,
        register_team_tools,
        register_worktree_tools,
    },
};
use synthia_job::TimeWheel;

use crate::{
    config::AppConfig,
    output::{CliEventHandler, OutputConfig},
};

pub async fn build_agent(
    config: &AppConfig,
    current_dir: &Path,
    model_override: Option<String>,
) -> Result<(Agent, AgentSetup)> {
    let question_sender = Arc::new(QuestionSenderImpl::new());
    let session_manager: Arc<dyn SessionManager> =
        Arc::new(SessionFileStore::new());
    let cron_store: Arc<CronFileStore> = Arc::new(CronFileStore::new());
    let output_config = OutputConfig::default();
    let time_wheel = Arc::new(TimeWheel::new());

    let setup = AgentSetup::new(
        question_sender.clone(),
        current_dir.to_owned(),
        session_manager,
        cron_store,
        output_config,
        time_wheel,
    );

    let agent = setup.build(config, model_override).await?;

    Ok((agent, setup))
}

pub struct AgentSetup {
    tool_registry: Arc<ToolRegistry>,
    question_sender: Arc<QuestionSenderImpl>,
    current_dir: PathBuf,
    session_manager: Arc<dyn SessionManager>,
    cron_store: Arc<CronFileStore>,
    output_config: OutputConfig,
    time_wheel: Arc<TimeWheel>,
    executor: Arc<LocalShellExecutor>,
}

impl AgentSetup {
    pub fn new(
        question_sender: Arc<QuestionSenderImpl>,
        current_dir: PathBuf,
        session_manager: Arc<dyn SessionManager>,
        cron_store: Arc<CronFileStore>,
        output_config: OutputConfig,
        time_wheel: Arc<TimeWheel>,
    ) -> Self {
        Self {
            tool_registry: Arc::new(ToolRegistry::new()),
            question_sender,
            current_dir,
            session_manager,
            cron_store,
            output_config,
            time_wheel,
            executor: Arc::new(LocalShellExecutor::new()),
        }
    }

    pub async fn register_tools(&self, agent: Agent) {
        register_builtin_tools(&self.tool_registry).await;
        self.tool_registry
            .register(Arc::new(AskUserQuestionTool::new(
                self.question_sender.clone(),
            )))
            .await;

        register_cron_tools(
            &self.tool_registry,
            self.cron_store.clone(),
            self.time_wheel.clone(),
            agent,
        )
        .await;

        register_task_tools(&self.tool_registry).await;

        register_team_tools(&self.tool_registry).await;
        register_worktree_tools(&self.tool_registry, self.current_dir.clone())
            .await;
        self.tool_registry
            .register(Arc::new(ExecTool::new(self.executor.clone())))
            .await;
        register_background_tools(&self.tool_registry, self.executor.clone())
            .await;
    }

    pub async fn register_mcp_tools(&self, config: &AppConfig) -> Result<()> {
        for (name, mcp_config) in config.get_mcps() {
            tracing::info!("Starting MCP server: {}", name);
            match start_mcp_server(&mcp_config).await {
                Ok(server_sink) => {
                    let tools = get_mcp_tools(server_sink).await?;
                    for tool in tools {
                        self.tool_registry
                            .register(std::sync::Arc::new(tool))
                            .await;
                    }
                    tracing::info!("Registered MCP tools for: {}", name);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to start MCP server {}: {}",
                        name,
                        e
                    );
                }
            }
        }
        Ok(())
    }

    pub async fn build(
        &self,
        config: &AppConfig,
        model_override: Option<String>,
    ) -> Result<synthia_agent::Agent> {
        let agent_config = AgentConfig {
            models: config.get_all_models(),
            ..Default::default()
        };

        let hook_registry = Arc::new(HookRegistry::new());
        hook_registry
            .register(
                Arc::new(LoggingHook::new()) as synthia_agent::hooks::HookPtr
            )
            .await;

        let skill_tool = Arc::new(SkillTool::new(self.current_dir.clone()));

        let mut models = config.get_all_models();
        if let Some(ref override_name) = model_override {
            if let Some(pos) = models
                .iter()
                .position(|m| m.model_info().name == *override_name)
            {
                let override_model = models.remove(pos);
                models.insert(0, override_model);
                tracing::info!("Model override applied: {}", override_name);
            } else {
                tracing::warn!(
                    "Model override '{}' not found in config, using default",
                    override_name
                );
            }
        }
        let model_router: Arc<dyn synthia_agent::model_router::ModelRouter> =
            Arc::new(FirstModelRouter::new(models));

        let session_manager = self.session_manager.clone();

        let context_manager =
            Arc::new(DefaultContextManager::new(model_router.clone()));

        // Set guardian on tool registry before creating AgentDeps
        let guardian = Arc::new(AdvancedGuardian::new(
            GuardianConfig::default(),
            model_router.clone(),
            context_manager.clone(),
        )) as Arc<dyn Guardian>;
        self.tool_registry.set_guardian(guardian).await;

        let agent = Agent::new(
            Arc::new(agent_config.clone()),
            synthia_agent::agent::AgentDeps {
                tools: self.tool_registry.clone(),
                context: context_manager.clone(),
                session: session_manager.clone(),
                router: model_router.clone(),
                hooks: hook_registry.clone(),
                skills: skill_tool.clone(),
                control: Arc::new(AgentControl::new()),
            },
        );

        self.register_tools(agent.clone()).await;
        self.register_mcp_tools(config).await?;

        let event_handler =
            Arc::new(CliEventHandler::new(self.output_config.clone()));

        let max_threads = config.get_max_agents();
        let guards = Arc::new(Guards::new(max_threads));

        let mut all_agents = config.get_agents(&self.current_dir);

        let built_in_agents = builtins::built_in::configs(&self.current_dir);
        for (name, agent_config) in built_in_agents {
            let agent_name =
                synthia_agent::config::AgentName::Custom(name.clone());
            if !all_agents.iter().any(|a| a.name == agent_name) {
                let config: AgentConfig = agent_config.clone();
                all_agents.push(config);
            }
        }

        let subagent_executor = SubagentExecutor::new(ExecutorConfig {
            tool_registry: self.tool_registry.clone(),
            context_manager: agent.deps.context.clone(),
            session_manager: agent.deps.session.clone(),
            model_router: agent.deps.router.clone(),
            hook_registry: agent.deps.hooks.clone(),
            skill_tool,
            event_handler,
            guards,
        });
        let subagent_tool = SubagentTool::new(Arc::new(subagent_executor))
            .with_configs(all_agents);
        self.tool_registry.register(Arc::new(subagent_tool)).await;

        Ok(agent)
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tool_registry.tool_names()
    }
}

async fn start_mcp_server(
    config: &crate::config::McpConfig,
) -> Result<rmcp::service::ServerSink> {
    use rmcp::{service::ServiceExt, transport::TokioChildProcess};

    let mut cmd = tokio::process::Command::new(&config.command);
    if let Some(args) = config.args.as_deref() {
        cmd.args(args);
    }

    for (key, value) in &config.env {
        cmd.env(key, value);
    }

    let client = ().serve(TokioChildProcess::new(cmd)?).await?;

    let peer = client.peer().clone();
    #[allow(clippy::disallowed_methods)]
    std::mem::forget(client);

    Ok(peer)
}
