//! `AppState`: top-level shared state container for the server.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use dashmap::DashMap;
use synthia_agent::{
    AgentEvent,
    build_default_tool_registry,
    control::{AgentControl, AgentRegistry as AgentControlRegistry},
    interceptor::InterceptorChain,
    tools::orchestrator::build_default_tool_orchestrator,
};
use synthia_command::CommandRegistry;
use synthia_core::tool::{
    builtin_fragments::SystemPromptFragment,
    extension_registry::ExtensionRegistry,
    fragment::FragmentRegistry,
    plugin_registry::PluginRegistry,
    registry::ToolRegistry as CoreToolRegistry,
    rollout::RolloutTracker,
    skill_registry::SkillRegistry,
};
use synthia_hook::HookRegistry;
use synthia_mcp::{McpManager, McpRegistry};
use synthia_permission::ApprovalService;
#[cfg(any(test, feature = "test-utils"))]
use synthia_permission::HeadlessApprovalService;
use synthia_provider::{
    config::WorkspaceConfig,
    router::ModelRouter,
    traits::ModelProvider,
};
#[cfg(any(test, feature = "test-utils"))]
use synthia_sandbox::NoopSandboxManager;
use synthia_sandbox::{SandboxManager, composite::CompositeSandboxManager};
use synthia_session::manager::SessionManager;
use synthia_tool::registry::ToolRegistry;
use synthia_tool_orchestrator::{DynamicResolver, ToolOrchestrator};
use tokio::sync::RwLock;

use super::{
    registry::AgentRegistry,
    subagent_factory::AppStateSubagentFactory,
};
use crate::{
    approval::{ApprovalState, HttpApprovalService},
    config::AuthConfig,
    event_stream::EventBroadcaster,
    mcp::McpService,
    scheduler::JobScheduler,
    session::controller::{
        AgentRunStreamFactory,
        RunDependencies,
        SessionController,
    },
};

pub struct AppState {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub hook_registry: Arc<RwLock<HookRegistry>>,
    pub session_manager: Arc<SessionManager>,
    pub command_registry: Arc<RwLock<CommandRegistry>>,
    pub model_router: Arc<RwLock<ModelRouter>>,
    pub mcp_manager: Arc<McpManager>,
    pub mcp_registry: Arc<McpRegistry>,
    pub mcp_module: Arc<McpService>,
    pub workspace_root: PathBuf,
    pub default_model: String,
    pub workspace_config: WorkspaceConfig,
    pub default_provider: Arc<dyn ModelProvider>,
    /// Authentication configuration shared with the auth middleware.
    /// `AuthLayer` reads `api_keys` / `key_to_user` from this and
    /// surfaces a `RequestUserId` extension on every request.
    pub auth_config: Arc<AuthConfig>,
    /// Per-session event broadcasters for SSE/WebSocket streaming,
    /// keyed by `(user_id, session_id)`.
    event_broadcasters:
        Arc<RwLock<HashMap<(String, String), EventBroadcaster>>>,
    /// Active session controllers keyed by `(user_id, session_id)`.
    pub active_sessions: Arc<DashMap<(String, String), Arc<SessionController>>>,
    /// Per-session agent state registry.
    pub agent_registry: Arc<AgentRegistry>,
    pub job_scheduler: Arc<JobScheduler>,
    /// Factory for creating child sessions from agent-side tools.
    pub subagent_factory: Arc<AppStateSubagentFactory>,
    /// Shared approval state backing the HTTP/WebSocket approval flow.
    pub approval_state: Arc<ApprovalState>,
    /// Approval service used by the tool orchestrator.
    pub approval_service: Arc<dyn ApprovalService>,
    /// Sandbox manager used by the tool orchestrator.
    pub sandbox_manager: Arc<dyn SandboxManager>,
    /// Dynamic resolver that allows runtime registration of discovered tools.
    pub tool_resolver: Arc<DynamicResolver>,
    /// Tool orchestrator that routes tool calls through approval and sandbox.
    pub tool_orchestrator: Arc<dyn ToolOrchestrator>,
    /// Unified extension registry aggregating FragmentRegistry, ToolRegistry, etc.
    pub extension_registry: ExtensionRegistry,
    /// Interceptor chain for cross-cutting concerns (permission, loop detect, etc.).
    pub interceptor_chain: Arc<InterceptorChain>,
    /// Rollout tracker for file changes and token usage tracking.
    pub rollout_tracker: Arc<RolloutTracker>,
    /// A2A protocol service (lazy-initialized).
    a2a_service: Arc<tokio::sync::OnceCell<crate::a2a::A2aService>>,
}

impl AppState {
    pub async fn new(workspace_root: PathBuf) -> Arc<Self> {
        let workspace_config =
            WorkspaceConfig::load_from_dir(&workspace_root).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to load workspace config, using env fallback");
                WorkspaceConfig::from_env()
            });

        let default_model = workspace_config.default_model.clone();

        let default_provider: Arc<dyn ModelProvider> = workspace_config
            .create_default_provider()
            .map(Arc::from)
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to create default provider");
                panic!("No LLM provider available. Configure .agents/config.toml or set OPENAI_API_KEY/ANTHROPIC_API_KEY environment variables.");
            });

        let session_manager =
            Arc::new(SessionManager::new(workspace_root.join("sessions")));
        let tool_registry = Arc::new(RwLock::new(build_default_tool_registry(
            workspace_root.clone(),
        )));
        let hooks = HookRegistry::new();
        let hook_registry = Arc::new(RwLock::new(hooks));
        let command_registry = {
            let reg = CommandRegistry::new();
            reg.register_builtins();
            Arc::new(RwLock::new(reg))
        };
        let model_router = Arc::new(RwLock::new(ModelRouter::new()));
        let mcp_manager = Arc::new(McpManager::new());
        let mcp_registry =
            Arc::new(McpRegistry::with_manager(mcp_manager.clone()));
        let mcp_module = Arc::new(McpService::new());

        let job_registry = Arc::new(crate::scheduler::JobRegistry::new());
        let job_scheduler = Arc::new(JobScheduler::new(job_registry));

        let auth_config = Arc::new(load_auth_config(&workspace_root));

        let approval_state = Arc::new(ApprovalState::new());
        let approval_service: Arc<dyn ApprovalService> =
            Arc::new(HttpApprovalService::new(approval_state.clone()));
        let sandbox_manager: Arc<dyn SandboxManager> = Arc::new(
            CompositeSandboxManager::default_linux(workspace_root.clone()),
        );
        let (tool_orchestrator, tool_resolver) =
            build_default_tool_orchestrator(
                workspace_root.clone(),
                approval_service.clone(),
                sandbox_manager.clone(),
            );

        // Build FragmentRegistry with built-in fragments
        let fragment_registry = Arc::new(FragmentRegistry::new());
        fragment_registry
            .register(Arc::new(SystemPromptFragment::new(
                "You are Synthia, an AI coding assistant. Help the user with their tasks.",
            )))
            .await
            .expect("failed to register SystemPromptFragment");

        // Build SkillRegistry with built-in skills
        let skill_registry = Arc::new(SkillRegistry::new());
        {
            use synthia_core::tool::builtin_skills::{
                CodingSkill,
                DebugSkill,
                SearchSkill,
            };
            skill_registry
                .register(Arc::new(CodingSkill)
                    as Arc<dyn synthia_core::tool::skill_registry::Skill>)
                .await
                .expect("failed to register CodingSkill");
            skill_registry
                .register(Arc::new(SearchSkill)
                    as Arc<dyn synthia_core::tool::skill_registry::Skill>)
                .await
                .expect("failed to register SearchSkill");
            skill_registry
                .register(Arc::new(DebugSkill)
                    as Arc<dyn synthia_core::tool::skill_registry::Skill>)
                .await
                .expect("failed to register DebugSkill");
        }

        // Build PluginRegistry (empty — plugins loaded on demand)
        let plugin_registry = Arc::new(PluginRegistry::new());

        // Build ExtensionRegistry (uses core ToolRegistry + FragmentRegistry + SkillRegistry + PluginRegistry)
        let core_tool_registry = Arc::new(CoreToolRegistry::new());
        let extension_registry = ExtensionRegistry::with_all(
            core_tool_registry,
            Arc::clone(&fragment_registry),
            Arc::clone(&skill_registry),
            Arc::clone(&plugin_registry),
        );

        // Build InterceptorChain with default guard
        let interceptor_chain =
            Arc::new(InterceptorChain::default_with_guard(None, None, None));

        // Build RolloutTracker
        let rollout_tracker = Arc::new(RolloutTracker::new());

        Arc::new_cyclic(|weak| Self {
            tool_registry,
            hook_registry,
            session_manager,
            command_registry,
            model_router,
            mcp_manager,
            mcp_registry,
            mcp_module,
            workspace_root,
            tool_resolver,
            default_model,
            workspace_config,
            default_provider,
            auth_config,
            event_broadcasters: Arc::new(RwLock::new(HashMap::new())),
            active_sessions: Arc::new(DashMap::new()),
            agent_registry: Arc::new(AgentRegistry::new()),
            job_scheduler,
            subagent_factory: Arc::new(AppStateSubagentFactory::new(
                weak.clone(),
            )),
            approval_state: approval_state.clone(),
            approval_service,
            sandbox_manager,
            tool_orchestrator,
            extension_registry,
            interceptor_chain,
            rollout_tracker,
            a2a_service: Arc::new(tokio::sync::OnceCell::new()),
        })
    }

    /// Create an AppState for testing with in-memory components and no LLM provider dependency.
    ///
    /// Unlike the production `new()`, this uses `FakeProvider` with empty
    /// responses and `HeadlessApprovalService`.  Registry-First components
    /// (FragmentRegistry, SkillRegistry, PluginRegistry, InterceptorChain)
    /// are initialized with the same built-in registrations as `new()` so
    /// that tests observe realistic wiring without needing external
    /// dependencies.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn for_test(
        session_manager: SessionManager,
        workspace_root: PathBuf,
    ) -> Self {
        let workspace_config = WorkspaceConfig::from_env();
        let default_model = workspace_config.default_model.clone();

        // Create a minimal provider that returns empty responses (for route-level tests)
        let default_provider: Arc<dyn ModelProvider> =
            Arc::new(test_support::FakeProvider::new(vec![]));

        let tool_registry = Arc::new(RwLock::new(build_default_tool_registry(
            workspace_root.clone(),
        )));
        let hook_registry = Arc::new(RwLock::new(HookRegistry::new()));
        let command_registry = Arc::new(RwLock::new(CommandRegistry::new()));
        let model_router = Arc::new(RwLock::new(ModelRouter::new()));
        let mcp_manager = Arc::new(McpManager::new());
        let mcp_registry =
            Arc::new(McpRegistry::with_manager(mcp_manager.clone()));
        let mcp_module = Arc::new(McpService::new());

        let job_registry = Arc::new(crate::scheduler::JobRegistry::new());
        let job_scheduler = Arc::new(JobScheduler::new(job_registry));

        let auth_config = Arc::new(load_auth_config(&workspace_root));

        let approval_state = Arc::new(ApprovalState::new());
        let approval_service: Arc<dyn ApprovalService> =
            Arc::new(HeadlessApprovalService);
        let sandbox_manager: Arc<dyn SandboxManager> =
            Arc::new(NoopSandboxManager);
        let (tool_orchestrator, tool_resolver) =
            build_default_tool_orchestrator(
                workspace_root.clone(),
                approval_service.clone(),
                sandbox_manager.clone(),
            );

        // Build FragmentRegistry with built-in SystemPromptFragment
        // (matches production AppState::new() registration)
        let fragment_registry = Arc::new(FragmentRegistry::new());
        fragment_registry
            .register(Arc::new(SystemPromptFragment::new(
                "You are Synthia, an AI coding assistant. Help the user with their tasks.",
            )))
            .await
            .expect("failed to register SystemPromptFragment");

        // Build SkillRegistry with built-in skills
        let skill_registry = Arc::new(SkillRegistry::new());
        {
            use synthia_core::tool::builtin_skills::{
                CodingSkill,
                DebugSkill,
                SearchSkill,
            };
            skill_registry
                .register(Arc::new(CodingSkill)
                    as Arc<dyn synthia_core::tool::skill_registry::Skill>)
                .await
                .expect("failed to register CodingSkill");
            skill_registry
                .register(Arc::new(SearchSkill)
                    as Arc<dyn synthia_core::tool::skill_registry::Skill>)
                .await
                .expect("failed to register SearchSkill");
            skill_registry
                .register(Arc::new(DebugSkill)
                    as Arc<dyn synthia_core::tool::skill_registry::Skill>)
                .await
                .expect("failed to register DebugSkill");
        }

        // Build PluginRegistry (empty — plugins loaded on demand)
        let plugin_registry = Arc::new(PluginRegistry::new());

        // Build ExtensionRegistry (mirrors production configuration)
        let core_tool_registry = Arc::new(CoreToolRegistry::new());
        let extension_registry = ExtensionRegistry::with_all(
            core_tool_registry,
            Arc::clone(&fragment_registry),
            Arc::clone(&skill_registry),
            Arc::clone(&plugin_registry),
        );

        // Build InterceptorChain with default guard (mirrors production)
        let interceptor_chain =
            Arc::new(InterceptorChain::default_with_guard(None, None, None));

        // Build RolloutTracker
        let rollout_tracker = Arc::new(RolloutTracker::new());

        Self {
            tool_registry,
            hook_registry,
            session_manager: Arc::new(session_manager),
            command_registry,
            model_router,
            mcp_manager,
            mcp_registry,
            mcp_module,
            workspace_root,
            tool_resolver,
            default_model,
            workspace_config,
            default_provider,
            auth_config,
            event_broadcasters: Arc::new(RwLock::new(HashMap::new())),
            active_sessions: Arc::new(DashMap::new()),
            agent_registry: Arc::new(AgentRegistry::new()),
            job_scheduler,
            // Test instances do not need a live back-reference to
            // themselves; sub-agent creation will fail gracefully if
            // the factory is invoked.
            subagent_factory: Arc::new(AppStateSubagentFactory::new(
                std::sync::Weak::new(),
            )),
            approval_state,
            approval_service,
            sandbox_manager,
            tool_orchestrator,
            extension_registry,
            interceptor_chain,
            rollout_tracker,
            a2a_service: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Gets or creates an `EventBroadcaster` for the given `(user_id,
    /// session_id)` pair.
    pub async fn get_or_create_broadcaster(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> EventBroadcaster {
        let key = (user_id.to_string(), session_id.to_string());
        let mut broadcasters = self.event_broadcasters.write().await;
        broadcasters
            .entry(key)
            .or_insert_with(EventBroadcaster::new)
            .clone()
    }

    /// Gets the event broadcaster for a session, if it exists.
    pub async fn get_event_broadcaster(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Option<EventBroadcaster> {
        let key = (user_id.to_string(), session_id.to_string());
        let broadcasters = self.event_broadcasters.read().await;
        broadcasters.get(&key).cloned()
    }

    /// Removes the event broadcaster for a session.
    pub async fn remove_broadcaster(&self, user_id: &str, session_id: &str) {
        let key = (user_id.to_string(), session_id.to_string());
        let mut broadcasters = self.event_broadcasters.write().await;
        broadcasters.remove(&key);
    }

    /// Gets or lazily initializes the A2A service.
    ///
    /// Uses `OnceCell` to ensure only one `A2aService` is created.
    /// The `Arc<Self>` is passed from the caller (route handler) because
    /// `&self` cannot produce an `Arc<Self>`.
    pub async fn a2a_service(
        self: &Arc<Self>,
        base_url: String,
    ) -> &crate::a2a::A2aService {
        self.a2a_service
            .get_or_init(|| async {
                crate::a2a::A2aService::new(self.clone(), base_url).await
            })
            .await
    }

    /// Gets or creates a [`SessionController`] for `(user_id, session_id)`,
    /// restoring the session from the on-disk store if it is not already
    /// loaded in memory.
    pub async fn get_or_create_session_controller(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Arc<SessionController>, crate::error::ServerError> {
        self.get_or_create_session_controller_with_parent(
            user_id, session_id, None, None,
        )
        .await
    }

    /// Gets or creates a [`SessionController`] for `(user_id, session_id)`
    /// with an optional parent event sender for subagent event forwarding
    /// and an optional parent spawn depth.
    ///
    /// When `parent_depth` is `Some(d)`, the created controller's
    /// sub-agent depth will be configured as `d + 1` so that
    /// nested spawns can enforce `max_depth`. When
    /// `None`, the controller is treated as a root session (depth 0).
    pub async fn get_or_create_session_controller_with_parent(
        &self,
        user_id: &str,
        session_id: &str,
        parent_event_sender: Option<tokio::sync::mpsc::Sender<AgentEvent>>,
        parent_depth: Option<usize>,
    ) -> Result<Arc<SessionController>, crate::error::ServerError> {
        let key = (user_id.to_string(), session_id.to_string());
        if let Some(entry) = self.active_sessions.get(&key) {
            return Ok(entry.clone());
        }

        if self.session_manager.get(session_id).await.is_none() {
            self.session_manager
                .restore(user_id, session_id)
                .await
                .map_err(|e| {
                    crate::error::ServerError::SessionError(format!(
                        "session '{session_id}' not found: {e}"
                    ))
                })?;
        }

        let child_depth = parent_depth.map(|d| d + 1).unwrap_or(0);
        let broadcaster =
            self.get_or_create_broadcaster(user_id, session_id).await;
        let queue = self.session_manager.input_queue();
        let session_path = self
            .session_manager
            .store()
            .session_dir(user_id, session_id);
        let deps = RunDependencies::new(
            Arc::clone(&self.default_provider),
            Arc::clone(&self.tool_registry),
            self.session_manager.store().clone(),
            self.workspace_root.clone(),
            self.default_model.clone(),
            Arc::clone(&self.subagent_factory)
                as Arc<dyn synthia_agent::SubagentSessionFactory>,
            Arc::clone(&self.approval_service),
            Arc::clone(&self.sandbox_manager),
            Arc::clone(&self.tool_orchestrator),
            Arc::new(AgentControl::new(Arc::new(AgentControlRegistry::new()))),
            child_depth,
        )
        .with_extension_registry(self.extension_registry.clone())
        .with_interceptor_chain(Arc::clone(&self.interceptor_chain))
        .with_rollout_tracker(Arc::clone(&self.rollout_tracker));

        let controller = SessionController::spawn(
            user_id,
            session_id,
            queue,
            session_path,
            broadcaster,
            deps,
            crate::session::controller::DEFAULT_IDLE_TIMEOUT,
            Arc::new(AgentRunStreamFactory),
            parent_event_sender,
        );

        self.active_sessions.insert(key, Arc::clone(&controller));
        Ok(controller)
    }
}

/// Load the [`AuthConfig`] for a workspace, with sensible fallbacks.
///
/// Order:
/// 1. `{workspace_root}/config.toml` if it exists (workspace config).
/// 2. `{workspace_root}/.synthia/config.toml` (alternate location).
/// 3. Environment override `SYNTHIA_AUTH__*` (not currently used).
/// 4. `AuthConfig::default()`.
fn load_auth_config(workspace_root: &std::path::Path) -> AuthConfig {
    use crate::config::ServerConfig;

    for candidate in [
        workspace_root.join("config.toml"),
        workspace_root.join(".synthia").join("config.toml"),
    ] {
        if candidate.exists() {
            match ServerConfig::load(&candidate) {
                Ok(cfg) => return cfg.auth,
                Err(e) => {
                    tracing::warn!(
                        path = %candidate.display(),
                        error = %e,
                        "failed to load server config; using default auth"
                    );
                }
            }
        }
    }
    AuthConfig::default()
}
