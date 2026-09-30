//! `AppState` construction: resolve the deployment config,
//! compose the tool surface, build the default agent, and
//! assemble every field. One concern — the boot.

use std::{path::PathBuf, sync::Arc};

use synthia::{
    core::{Clock, registry::Registry},
    harness::{AgentEntry, AgentRegistry},
    provider::{config::WorkspaceConfig, traits::ModelProvider},
    session::{jsonl_session_search, manager::SessionRegistry},
    skill::{register_skill_tool, seed_default_skills},
    tool::ToolRegistry,
    tool_task::TaskDelegator,
};
use tokio::sync::RwLock;

use super::{
    AppState,
    EvalsState,
    ProviderCatalogue,
    SchedulesState,
    WorkflowsState,
    tools::plugin_tool_registry,
    usage::UsageMetrics,
};
use crate::{
    config::{AuthConfig, CorsConfig},
    state::boot::{
        apply_tools_config,
        build_prompt_context,
        load_default_compaction,
        load_default_max_iterations,
        load_default_strategy,
        load_default_tool_restriction,
        register_configured_mcp_servers,
        warn_unapplied_agent_keys,
    },
};

/// Resolve the runtime [`WorkspaceConfig`] from the optional `--config`
/// override first, falling back to the legacy
/// `<workspace>/.agents/config.toml` path when no override is given or
/// the override fails to load.
fn load_workspace_config(
    workspace_root: &std::path::Path,
    config_path: Option<&std::path::PathBuf>,
) -> Result<WorkspaceConfig, anyhow::Error> {
    let Some(path) = config_path else {
        return legacy_workspace_config(workspace_root);
    };
    match bridged_workspace_config(path) {
        Ok(cfg) => Ok(cfg),
        Err(reason) => {
            tracing::warn!(path = %path.display(), reason, "falling back");
            legacy_workspace_config(workspace_root)
        }
    }
}

/// One attempt at loading the `--config` YAML as a
/// [`WorkspaceConfig`]. `Err` carries the human-readable
/// reason (missing / unsupported / parse failure) so the
/// caller logs one line and falls back.
fn bridged_workspace_config(
    path: &std::path::Path,
) -> Result<WorkspaceConfig, String> {
    use crate::config::yaml_bridge;
    match yaml_bridge::load_yaml_config_as_workspace_config(path) {
        Ok(Some(cfg)) => {
            tracing::info!(
                path = %path.display(),
                providers = ?cfg.providers.keys().collect::<Vec<_>>(),
                "loaded provider configuration from --config"
            );
            Ok(cfg)
        }
        Ok(None) => Err("--config path missing or unsupported".to_string()),
        Err(e) => Err(format!("failed to bridge --config: {e}")),
    }
}

/// `load_workspace_config`'s legacy fallback: the
/// `<workspace>/.agents/config.toml` path used when the
/// `--config` override is absent, unsupported, or fails.
fn legacy_workspace_config(
    workspace_root: &std::path::Path,
) -> Result<WorkspaceConfig, anyhow::Error> {
    WorkspaceConfig::load_from_dir(workspace_root)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

impl AppState {
    pub async fn new(
        workspace_root: PathBuf,
        config_path: Option<&std::path::PathBuf>,
    ) -> Result<Arc<Self>, String> {
        let server_config =
            crate::config::resolve_server_config(&workspace_root, config_path);
        Self::with_server_config(workspace_root, config_path, server_config)
            .await
    }

    /// [`AppState::new`] with the deployment configuration already
    /// resolved by the caller.
    ///
    /// The CLI needs the resolved config *before* any state exists — to
    /// compute the bind address from `--host`/`SYNTHIA_HOST`/`host`
    /// precedence (R54) — so it resolves once and hands the value in
    /// rather than letting this function read the file a second time.
    pub async fn with_server_config(
        workspace_root: PathBuf,
        config_path: Option<&std::path::PathBuf>,
        server_config: Option<crate::config::ServerConfig>,
    ) -> Result<Arc<Self>, String> {
        let workspace_config = load_workspace_config(&workspace_root, config_path)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to load workspace config, using env fallback");
                WorkspaceConfig::from_env()
            });

        // R55: `agents.<name>` accepts keys the boot does not apply
        // (`description`, `model`, `allowed_tools`, `denied_tools`,
        // `hidden`, `color`). Silence is the bug — an operator who wrote
        // `denied_tools = ["shell"]` believes the tool is gone. Name the
        // keys, once per agent, at boot.
        if let Some(config) = server_config.as_ref() {
            warn_unapplied_agent_keys(config);
        }

        let default_model = workspace_config.default_model.clone();

        // Build every configured provider once, here: a turn then
        // resolves a name to a handle without touching the
        // environment, and the boot's failure mode stays explicit —
        // the *default* provider must build, a secondary one may not.
        let provider_catalogue =
            ProviderCatalogue::from_workspace_config(&workspace_config);
        let default_provider: Arc<dyn ModelProvider> =
            provider_catalogue.default_handle().map_err(|e| {
                tracing::error!(error = %e, "Failed to create default provider");
                format!(
                    "No LLM provider available: {e}. Configure .agents/config.toml or set \
                     OPENAI_API_KEY/ANTHROPIC_API_KEY environment variables."
                )
            })?;

        // One clock for the whole process: every timestamped
        // component (session log rows, operation snapshots, the
        // attachment store, the MCP supervisor's tick) reads it from
        // here rather than calling the wall clock itself.
        let clock = synthia::core::SharedClock::system();

        let session_search =
            jsonl_session_search(workspace_root.join("sessions"));
        let session_manager =
            Arc::new(SessionRegistry::new(workspace_root.join("sessions")));
        // The default tool surface, composed from the plugin
        // crates, then the agent-facing `skill` tool on top. The
        // skill implementation lives in `synthia-skill` so the HTTP
        // list route and the LLM-facing tool share one home;
        // registering before wrapping in `RwLock` lets the
        // synchronous `register_skill_tool(&registry)` call take
        // `&ToolRegistry` directly without going through an async
        // lock guard.
        let tool_registry_built = plugin_tool_registry();
        register_skill_tool(&tool_registry_built);
        // R21/R30: connect the configured MCP servers through the
        // supervisor and publish their tools. A server that fails
        // to start is logged (with its health state) and skipped —
        // boot must never be blocked by an optional integration.
        let (mcp_clients, mcp_health) = match server_config.as_ref() {
            Some(config) => {
                register_configured_mcp_servers(
                    &tool_registry_built,
                    clock.now(),
                    config,
                )
                .await
            }
            None => (Vec::new(), Vec::new()),
        };
        // R34: apply the deployment's `[tools]` surface after every
        // registration source (builtins, skill tool, MCP tools) has had
        // its say, so the config can name remote tools too. Unknown
        // names are warnings, never boot failures — see
        // `apply_tools_config`.
        let tool_surface = apply_tools_config(
            &tool_registry_built,
            &server_config
                .as_ref()
                .map(|c| c.tools.clone())
                .unwrap_or_default(),
        );
        // Smaller per-request load: with cross-domain search wired
        // below, MCP tools stop carrying their full JSON schemas on
        // every completion request — they advertise name +
        // description (`ToolExposure::Deferred`) and the schema
        // promotes on the first call. Only `Direct` flips, so an
        // operator's explicit `deferred` / `hidden` verdict stands.
        for d in tool_registry_built.descriptors().iter().filter(|d| {
            d.name.starts_with("mcp__")
                && d.exposure == synthia::tool::ToolExposure::Direct
        }) {
            tool_registry_built
                .set_exposure(&d.name, synthia::tool::ToolExposure::Deferred);
        }
        let tool_surface_policy = tool_surface.policy();
        let tool_registry = Arc::new(RwLock::new(tool_registry_built));
        // Per-section config: auth, CORS, operations, default
        // agent name, max iterations, compaction, restriction,
        // strategy. The eight fields group by what the boot does
        // with them, not by where they sit in the file.
        // Extracting the eight near-identical `cfg?` chains
        // keeps the orchestrator flat: the reader scans one
        // list and one shape at a time, instead of repeating
        // the chain eight times.
        let PerSectionConfig {
            auth_config,
            cors_config,
            operations_config,
            default_agent_name_value,
            default_max_iterations,
            default_compaction,
            default_tool_restriction,
            default_strategy,
        } = Self::load_per_section_config(&server_config);

        // Default system prompt. The ReAct loop builds its
        // own system prompt per-dispatch via
        // `synthia::harness::PromptContext::assemble`, so
        // this is only the legacy/default fallback for
        // callers that read `AppState::system_prompt`
        // directly without an explicit descriptor.
        let system_prompt = synthia::harness::DEFAULT_SYSTEM_PROMPT.to_string();

        // Seed the workflow skills on disk so the v1 HTTP
        // `GET /api/v1/skills` endpoint — which scans the workspace's
        // `.agents/skills/` directory — has visible data on a fresh
        // checkout. Existing user-installed skills are never
        // overwritten.
        if let Err(err) = seed_default_skills(&workspace_root) {
            tracing::warn!(error = %err, "failed to seed default skills");
        }

        // Build the multi-agent registry, the server-wide
        // steering policy, and register the default ReAct agent.
        // Extracted so the orchestrator stays flat — the
        // largest block in `with_server_config` (50+ lines
        // threading tool registry / provider / policy /
        // restriction / strategy through the
        // `ReActAgent::with_*` builder chain, plus a
        // `try_read()` snapshot and four `if let Some` chains)
        // lives in its own private fn.
        let steering = Arc::new(synthia::steering::Steering::default_policy(
            workspace_root.clone(),
        ));
        let agent_registry = Self::build_default_agent(
            &default_provider,
            &tool_registry,
            tool_surface_policy,
            default_max_iterations,
            default_strategy.clone(),
            default_tool_restriction.clone(),
            steering.clone(),
        )
        .await;

        // Tool schemas travel on the completion request's
        // `tools` channel, not in the prompt — the runtime's
        // `ToolRegistry` is the source of truth and the live
        // snapshot is computed inside the ReAct loop on each
        // call. The prompt context here therefore only carries
        // skills + peer agents.

        // Build the prompt-context manifest from the workspace
        // and the live agent registry, excluding the canonical
        // ReAct agent itself (it must not appear in its own
        // handoff manifest). All skills are enabled.
        let prompt_context = Arc::new(
            build_prompt_context(&workspace_root, &agent_registry, "agent")
                .await,
        );

        // Multimodal attachment store (R11). Build before the
        // `Arc::new_cyclic` closure because `workspace_root`
        // moves into the struct initializer below.
        let attachments_dir = workspace_root.join(".attachments");
        let attachment_store = Arc::new(
            synthia::attachment::AttachmentStore::new_fs(&attachments_dir)
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        error = %e,
                        "failed to initialise attachments dir; using in-memory store"
                    );
                    synthia::attachment::AttachmentStore::default()
                })
                .with_clock(clock.clone()),
        );

        // Long-term memory: files under the workspace's nearest
        // scopes (`.synthia/memory-local` writes, `.synthia/memory`
        // as a checked-in overlay). A failed construction (path
        // guards) degrades to the in-memory tier rather than
        // blocking boot — same contract as every optional
        // integration above.
        let memory: Arc<dyn synthia::context::Memory> =
            match synthia::context::FileMemory::new(
                synthia::context::FileMemoryConfig::new(
                    &workspace_root,
                    "synthia",
                )
                .with_scopes(vec![
                    synthia::context::MemoryScope::Local,
                    synthia::context::MemoryScope::Project,
                ]),
            ) {
                Ok(m) => Arc::new(m),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to open file memory; using in-memory tier"
                    );
                    Arc::new(synthia::context::InMemoryMemory::new())
                }
            };

        // The cross-domain search engine over six domains — the
        // same registry serves the model-facing `search` tool
        // (capability 1) and `GET /api/v1/search` (capability 2).
        let search =
            crate::search::SearchService::build(crate::search::SearchSources {
                tool_registry: Arc::clone(&tool_registry),
                agent_registry: Arc::clone(&agent_registry),
                workspace_root: workspace_root.clone(),
                memory: Arc::clone(&memory),
                session_search: Arc::clone(&session_search),
                clock: clock.clone(),
            });
        // Capability 1: the agent's discovery tool. Deferred
        // exposure — the cold-start tool list carries name +
        // description, and the real schema promotes on the first
        // call.
        {
            let g = tool_registry.read().await;
            if !synthia::tool_search::register_search_tool(
                &g,
                search.registry(),
            ) {
                tracing::warn!(
                    "failed to register the search tool: name shadowed"
                );
            }
        }

        // The four HTTP-backed management subsystems. Schedules
        // use the existing `ScheduleStore`; workflows / evals are
        // in-memory this turn (BTreeMaps behind a Tokio RwLock),
        // mirroring the existing SearchService's pattern.
        let schedules = SchedulesState::build(&workspace_root);
        let workflows = WorkflowsState::build();
        let evals = EvalsState::build();

        // Sub-agent session router over the same registry + cache
        // the HTTP surface uses: delegated children get sessions
        // exactly like user conversations (one code path for both).
        let active_sessions = Arc::new(crate::state::ActiveSessions::new());
        let subagent_router =
            Arc::new(crate::session::subagent_router::SubagentRouter::new(
                Arc::clone(&session_manager),
                Arc::clone(&active_sessions),
                clock.clone(),
            ));
        // The state handle is bound before the self-management
        // registration below — its tools close over the Arc.
        let state = Arc::new_cyclic(|_weak| Self {
            clock,
            tool_registry,
            agent_registry,
            session_search,
            session_manager,
            workspace_root,
            default_model,
            workspace_config,
            default_provider,
            provider_catalogue: Arc::new(provider_catalogue),
            auth_config,
            cors_config,
            operations_config,
            system_prompt,
            prompt_context,
            steering,
            server_config,
            default_max_iterations,
            default_compaction,
            default_strategy,
            default_tool_restriction,
            tool_surface,
            tool_defs_cache: parking_lot::RwLock::new(None),
            mcp_clients,
            mcp_health,
            active_sessions,
            subagent_router,
            default_agent_name: parking_lot::RwLock::new(
                default_agent_name_value,
            ),
            usage_metrics: Arc::new(UsageMetrics::default()),
            attachment_store,
            memory,
            search,
            schedules,
            workflows,
            evals,
        });
        // The self-management MCP server needs the finished state
        // handle (its tools call back into it), so it registers
        // after construction — the shared registry makes the tools
        // visible to every later run.
        super::self_mcp::register_self_mcp(&state).await;
        Ok(state)
    }
}

/// Output of `AppState::load_per_section_config`: every
/// per-section config block the orchestrator hands to
/// `AppState::with_server_config`'s final `Arc::new_cyclic`
/// block. Private — the boot is the only caller.
struct PerSectionConfig {
    auth_config: Arc<AuthConfig>,
    cors_config: Arc<CorsConfig>,
    operations_config: Arc<crate::config::OperationEndpointConfig>,
    default_agent_name_value: Option<String>,
    default_max_iterations: usize,
    default_compaction: Option<synthia::context::CompactionSettings>,
    default_tool_restriction: Option<Arc<synthia::tool::ToolRestriction>>,
    default_strategy:
        Option<Arc<dyn synthia::harness::agent::ReasoningStrategy>>,
}

impl AppState {
    /// Read every per-section config block the agent table
    /// needs. The eight fields group by what the boot does
    /// with them: auth / CORS / operations go straight into
    /// `AppState`; `default_agent_name_value` and the four
    /// `load_default_*` readers wire into the default agent
    /// and every run the session factory builds.
    ///
    /// Each reader follows the same shape (`cfg?` →
    /// `agents.get(name)?` → field read → fallback). Grouping
    /// them keeps the orchestrator's cognitive complexity
    /// flat: the reader scans one list and one shape at a
    /// time, instead of repeating the `cfg?` chain eight
    /// times across the boot.
    fn load_per_section_config(
        server_config: &Option<crate::config::ServerConfig>,
    ) -> PerSectionConfig {
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
        let default_agent_name_value =
            server_config.as_ref().and_then(|c| c.default_agent.clone());
        // Resolve the configured iteration cap from
        // `agents.<default>.max_steps` (clamped to `[1, 4096]`).
        // Falls back to the ReAct default (25) when no config
        // exists, no default agent is named, or the field is
        // unset on the chosen agent.
        let default_max_iterations = load_default_max_iterations(
            server_config.as_ref(),
            default_agent_name_value.as_deref(),
        )
        .unwrap_or(synthia::harness::DEFAULT_MAX_ITERATIONS);

        // R16: resolve the optional compaction policy from
        // `agents.<default>.compaction`. `None` keeps the loop's
        // default context manager.
        let default_compaction = load_default_compaction(
            server_config.as_ref(),
            default_agent_name_value.as_deref(),
        );

        // R58: resolve the agent's own allow/deny list from
        // `agents.<default>.{allowed_tools,denied_tools}`. An
        // empty pair means no restriction (every advertised
        // tool stays callable).
        let default_tool_restriction = load_default_tool_restriction(
            server_config.as_ref(),
            default_agent_name_value.as_deref(),
        );

        // R50: resolve the configured reasoning loop from
        // `agents.<default>.strategy`. An unknown name is
        // logged and treated as unset (ReAct) — a typo in one
        // agent's config must not stop the server from booting.
        let default_strategy = load_default_strategy(
            server_config.as_ref(),
            default_agent_name_value.as_deref(),
        );

        PerSectionConfig {
            auth_config,
            cors_config,
            operations_config,
            default_agent_name_value,
            default_max_iterations,
            default_compaction,
            default_tool_restriction,
            default_strategy,
        }
    }

    /// Build the multi-agent registry and register the default
    /// ReAct agent. Pulled out of `with_server_config` because
    /// the original was the largest single block in the boot —
    /// ~50 lines threading tool registry / provider / policy /
    /// restriction / strategy through the `ReActAgent::with_*`
    /// builder chain, plus a `try_read()` snapshot and four
    /// `if let Some` chains. In its own private fn the
    /// orchestrator stays at "wire → assemble".
    ///
    /// The steering bundle is passed in (the orchestrator
    /// builds it once; the `AppState` field and the default
    /// agent share the same `Arc`).
    async fn build_default_agent(
        default_provider: &Arc<dyn ModelProvider>,
        tool_registry: &Arc<RwLock<ToolRegistry>>,
        tool_surface_policy: Option<synthia::tool::ToolSurfacePolicy>,
        default_max_iterations: usize,
        default_strategy: Option<
            Arc<dyn synthia::harness::agent::ReasoningStrategy>,
        >,
        default_tool_restriction: Option<Arc<synthia::tool::ToolRestriction>>,
        steering: Arc<synthia::steering::Steering>,
    ) -> Arc<AgentRegistry> {
        let agent_registry = Arc::new(AgentRegistry::new());
        // Snapshot the tool registry for `ReActAgent::new`. The
        // agent captures the registry it was built with; the
        // per-policy installers below carry the configured
        // surface / strategy / restriction.
        let tool_registry_snapshot = {
            let g = tool_registry.try_read();
            match g {
                Ok(g) => Arc::new(g.clone()),
                Err(_) => Arc::new(ToolRegistry::new()),
            }
        };
        let mut default_agent = synthia::harness::ReActAgent::new(
            Arc::clone(default_provider),
            tool_registry_snapshot,
        )
        // The default member carries the delegation plugin so
        // `task` calls resolve the same catalog the session
        // factory builds. (Its prompt manifest is assembled
        // once `prompt_context` exists, after this call.)
        .with_interceptor(Arc::new(TaskDelegator::new(Arc::clone(
            &agent_registry,
        ))))
        .with_steering(steering)
        .with_max_iterations(default_max_iterations);
        // R34: the default registry member advertises the
        // configured surface as well, so it agrees with every
        // run the factory builds.
        if let Some(policy) = tool_surface_policy {
            default_agent = default_agent.with_tool_surface(policy);
        }
        // R50: same idea for the reasoning loop.
        if let Some(strategy) = default_strategy {
            default_agent = default_agent.with_strategy(strategy);
        }
        // R58: and for the agent's allow/deny list.
        if let Some(restriction) = default_tool_restriction {
            default_agent =
                default_agent.with_tool_restriction((*restriction).clone());
        }
        if let Err(e) = agent_registry
            .put(AgentEntry::new(Arc::new(default_agent)))
            .await
        {
            tracing::warn!(
                error = %e,
                "failed to register default agent; proceeding without it"
            );
        }
        agent_registry
    }
}
