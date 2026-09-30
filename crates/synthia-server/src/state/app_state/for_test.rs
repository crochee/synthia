//! The `test-utils` constructor: an `AppState` with in-memory
//! components and a fake provider, mirroring the production
//! wiring so route-level tests observe realistic state.

use std::{path::PathBuf, sync::Arc};

use dashmap::DashMap;
use synthia::{
    core::registry::Registry,
    harness::{AgentEntry, AgentRegistry},
    provider::{config::WorkspaceConfig, traits::ModelProvider},
    session::{jsonl_session_search, manager::SessionRegistry},
    skill::register_skill_tool,
    tool::ToolRegistry,
};
use tokio::sync::RwLock;

use super::{
    AppState,
    EvalsState,
    ProviderCatalogue,
    SchedulesState,
    WorkflowsState,
    tools::{AppliedToolSurface, plugin_tool_registry},
    usage::UsageMetrics,
};
use crate::{
    session::controller::SessionController,
    state::boot::build_prompt_context,
};

impl AppState {
    /// Create an AppState for testing with in-memory components and no LLM provider dependency.
    ///
    /// Unlike the production `new()`, this uses `FakeProvider` with empty
    /// responses. FragmentRegistry and SkillRegistry are initialized with
    /// the same built-in registrations as `new()` so that tests observe
    /// realistic wiring without needing external dependencies.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn for_test(
        session_manager: SessionRegistry,
        workspace_root: PathBuf,
    ) -> Self {
        let workspace_config = WorkspaceConfig::from_env();
        let default_model = workspace_config.default_model.clone();

        // Create a minimal provider that returns empty responses (for route-level tests)
        let default_provider: Arc<dyn ModelProvider> =
            Arc::new(synthia::test_support::FakeProvider::new(vec![]));

        // The catalogue every resolution path reads. A fixture builds
        // its own over whatever providers it wires (see
        // `ProviderCatalogue::with_handle`); this default one offers
        // exactly the fake, under the workspace's configured default
        // provider name, so `model: None` and an explicit selection of
        // that name both resolve.
        let provider_catalogue = ProviderCatalogue::default().with_handle(
            &workspace_config.default_provider,
            &default_model,
            Arc::clone(&default_provider),
        );

        let tool_registry_built = plugin_tool_registry();
        // Mirror the production wiring: the skill tool is part
        // of the default tool surface, not an opt-in extension.
        register_skill_tool(&tool_registry_built);
        let tool_registry = Arc::new(RwLock::new(tool_registry_built));

        // Same resolution as `new()`, without an explicit path: a test
        // that writes `config.toml` into its tempdir observes the same
        // sections the server would.
        let server_config =
            crate::config::resolve_server_config(&workspace_root, None);
        let auth_config = Arc::new(
            server_config
                .as_ref()
                .map(|c| c.auth.clone())
                .unwrap_or_default(),
        );
        let cors_config = Arc::new(
            server_config
                .as_ref()
                .map(|c| c.cors.clone())
                .unwrap_or_default(),
        );
        let operations_config = Arc::new(
            server_config
                .as_ref()
                .map(|c| c.operations.clone())
                .unwrap_or_default(),
        );

        let agent_registry = Arc::new(AgentRegistry::new());
        // Test state: register the canonical ReAct descriptor
        // (without holding a live provider) so the registry has
        // at least one entry and the peer-agent manifest snapshot
        // excludes it via the `self_name` filter.
        let _ = agent_registry
            .put(AgentEntry::new(Arc::new(
                synthia::harness::ReActAgent::new(
                    Arc::clone(&default_provider),
                    Arc::new(ToolRegistry::new()),
                ),
            )))
            .await;
        let prompt_context = Arc::new(
            build_prompt_context(&workspace_root, &agent_registry, "agent")
                .await,
        );

        let clock = synthia::core::SharedClock::system();
        let session_search =
            jsonl_session_search(workspace_root.join("sessions"));
        // Mirror the production wiring: an in-memory long-term
        // tier (tests seed entries through `Memory::store`) and
        // the cross-domain search service over it.
        let memory: Arc<dyn synthia::context::Memory> =
            Arc::new(synthia::context::InMemoryMemory::new());
        let search =
            crate::search::SearchService::build(crate::search::SearchSources {
                tool_registry: Arc::clone(&tool_registry),
                agent_registry: Arc::clone(&agent_registry),
                workspace_root: workspace_root.clone(),
                memory: Arc::clone(&memory),
                session_search: Arc::clone(&session_search),
                clock: clock.clone(),
            });
        {
            let g = tool_registry.read().await;
            synthia::tool_search::register_search_tool(&g, search.registry());
        }

        // The three management subsystems under test. Schedules
        // use the real `ScheduleStore` rooted at the test
        // workspace; workflows / evals are empty in-memory
        // registries — tests populate them through the route
        // handlers they're verifying.
        let schedules = SchedulesState::build(&workspace_root);
        let workflows = WorkflowsState::build();
        let evals = EvalsState::build();

        let session_manager = Arc::new(session_manager);
        let active_sessions: Arc<
            DashMap<(String, String), Arc<SessionController>>,
        > = Arc::new(DashMap::new());
        let subagent_router =
            Arc::new(crate::session::subagent_router::SubagentRouter::new(
                Arc::clone(&session_manager),
                Arc::clone(&active_sessions),
                clock.clone(),
            ));

        Self {
            clock,
            tool_registry,
            agent_registry,
            session_search,
            session_manager,
            workspace_root: workspace_root.clone(),
            default_model,
            workspace_config,
            default_provider,
            provider_catalogue: Arc::new(provider_catalogue),
            auth_config,
            cors_config,
            operations_config,
            system_prompt: synthia::harness::DEFAULT_SYSTEM_PROMPT.to_string(),
            prompt_context,
            steering: Arc::new(synthia::steering::Steering::default_policy(
                workspace_root.clone(),
            )),
            server_config,
            default_max_iterations: synthia::harness::DEFAULT_MAX_ITERATIONS,
            default_compaction: None,
            default_strategy: None,
            default_tool_restriction: None,
            tool_surface: AppliedToolSurface::default(),
            tool_defs_cache: parking_lot::RwLock::new(None),
            mcp_clients: Vec::new(),
            mcp_health: Vec::new(),
            active_sessions,
            subagent_router,
            default_agent_name: parking_lot::RwLock::new(None),
            usage_metrics: Arc::new(UsageMetrics::default()),
            attachment_store: Arc::new(
                synthia::attachment::AttachmentStore::new_fs(
                    workspace_root.join(".attachments"),
                )
                .unwrap_or_else(|_| {
                    synthia::attachment::AttachmentStore::default()
                }),
            ),
            memory,
            search,
            schedules,
            workflows,
            evals,
        }
    }
}
