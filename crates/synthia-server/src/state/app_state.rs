//! `AppState`: top-level shared state container for the server.

use std::{path::PathBuf, sync::Arc};

use dashmap::DashMap;
use synthia_agent::{AgentEntry, AgentEvent, AgentRegistry, PromptContext};
use synthia_core::registry::{Registry, RegistryItem};
use synthia_provider::{config::WorkspaceConfig, traits::ModelProvider};
use synthia_session::manager::SessionRegistry;
use synthia_skill::{register_skill_tool, seed_default_skills};
use synthia_tool::{build_default_tool_registry, registry::ToolRegistry};
use tokio::sync::RwLock;

use crate::{
    config::{AuthConfig, CorsConfig},
    session::controller::{
        AgentRunStreamFactory,
        RunDependencies,
        SessionController,
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
    use crate::config::yaml_bridge;

    if let Some(path) = config_path {
        match yaml_bridge::load_yaml_config_as_workspace_config(path) {
            Ok(Some(cfg)) => {
                tracing::info!(
                    path = %path.display(),
                    providers = ?cfg.providers.keys().collect::<Vec<_>>(),
                    "loaded provider configuration from --config"
                );
                return Ok(cfg);
            }
            Ok(None) => {
                tracing::warn!(
                    path = %path.display(),
                    "--config path missing or unsupported; falling back"
                );
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to bridge --config; falling back to legacy path"
                );
            }
        }
    }

    WorkspaceConfig::load_from_dir(workspace_root)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

pub struct AppState {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Multi-agent registry. Populated at startup with the
    /// canonical [`ReActAgent`] descriptor so any panel/judge
    /// agents registered later surface in the system prompt as
    /// handoff targets. The `self` agent is excluded from its
    /// own peer-agent manifest.
    pub agent_registry: Arc<AgentRegistry>,
    pub session_manager: Arc<SessionRegistry>,
    pub workspace_root: PathBuf,
    pub default_model: String,
    pub workspace_config: WorkspaceConfig,
    pub default_provider: Arc<dyn ModelProvider>,
    /// Authentication configuration shared with the auth middleware.
    /// `AuthLayer` reads `api_keys` / `key_to_user` from this and
    /// surfaces a `RequestUserId` extension on every request.
    pub auth_config: Arc<AuthConfig>,
    /// CORS configuration shared with the router.
    /// Read by `build_cors_layer` to attach a `CorsLayer` to the
    /// `Router` so browser clients (e.g. synthia-web on
    /// http://localhost:5173) can call the API endpoints.
    pub cors_config: Arc<CorsConfig>,
    /// Per-session event broadcasters live on the
    /// [`SessionController`] itself; SSE/WebSocket subscribers
    /// obtain a [`broadcast::Receiver`] via
    /// [`SessionController::subscribe`].
    /// Active session controllers keyed by `(user_id, session_id)`.
    pub active_sessions: Arc<DashMap<(String, String), Arc<SessionController>>>,
    /// Pre-rendered default system prompt. The agent loop
    /// currently builds its own prompt from the descriptor
    /// via [`synthia_agent::prompt::PromptContext::assemble`],
    /// so this field exists as a lock-free default for
    /// legacy callers that read it directly. Stored on
    /// `AppState` so the read is cheap on every dispatch.
    pub system_prompt: String,
    /// Pre-built prompt manifest (skills + peer agents + tool
    /// definitions) consumed by the
    /// [`synthia_agent::prompt::PromptContext::assemble`]
    /// method. Built once at startup and shared via `Arc` into
    /// every [`crate::session::controller::RunDependencies`] —
    /// the manifest never mutates after boot, so wrapping it
    /// in an `Arc` lets every dispatch and every per-agent
    /// `build_react_agent` call share the same allocation
    /// instead of deep-cloning the full skills + peer-agent
    /// list on each one.
    pub prompt_context: Arc<synthia_agent::prompt::PromptContext>,
    /// Configured default agent name. Loaded from
    /// `config.agent.default` at startup. `None` falls back to
    /// the first agent registered in [`Self::agent_registry`].
    ///
    /// Backed by [`parking_lot::RwLock`] (not
    /// [`tokio::sync::RwLock`]) so the run factory can resolve
    /// the default agent name synchronously without awaiting.
    pub default_agent_name: parking_lot::RwLock<Option<String>>,
    /// Process-wide usage counters surfaced by
    /// `GET /api/v1/chat/usage`. Mutated by the chat REST
    /// handlers as turns complete; read-only via
    /// [`AppState::usage_metrics`].
    pub usage_metrics: UsageMetrics,
}

/// Process-wide usage counters. Cheap to read (lock-free
/// `std::sync::atomic` snapshot) so the `/api/v1/chat/usage`
/// endpoint can poll without contention.
#[derive(Debug, Default)]
pub struct UsageMetrics {
    pub tokens_in: std::sync::atomic::AtomicU64,
    pub tokens_out: std::sync::atomic::AtomicU64,
    pub turns: std::sync::atomic::AtomicU64,
}

impl UsageMetrics {
    /// Atomic snapshot of the three counters. Field order in
    /// `UsageResponse` mirrors this struct's order — `tokens_in`
    /// before `tokens_out` before `turns`.
    pub fn snapshot(&self) -> UsageSnapshot {
        UsageSnapshot {
            tokens_in: self
                .tokens_in
                .load(std::sync::atomic::Ordering::Relaxed),
            tokens_out: self
                .tokens_out
                .load(std::sync::atomic::Ordering::Relaxed),
            turns: self.turns.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// Plain-data snapshot of [`UsageMetrics`] for the API
/// response. Cheap to clone via `Copy` since the underlying
/// counters are already integers.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct UsageSnapshot {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub turns: u64,
}

use serde::Serialize;

impl AppState {
    /// Resolve an agent by name through the standard ladder:
    /// `explicit > configured default > first registered`.
    ///
    /// Uses [`AgentRegistry::resolve_sync`] under the
    /// [`parking_lot::RwLock`] so callers in synchronous contexts
    /// (e.g. `SessionController::build_run_config`) can resolve
    /// the running agent without `await`.
    pub fn resolve_agent_name(
        registry: &AgentRegistry,
        default_agent_name: Option<&str>,
        explicit: Option<&str>,
    ) -> Option<String> {
        if let Some(name) = explicit
            && let Some(arc) = registry.resolve_sync(name)
        {
            return Some(arc.descriptor().name.clone());
        }
        if let Some(default_name) = default_agent_name
            && let Some(arc) = registry.resolve_sync(default_name)
        {
            return Some(arc.descriptor().name.clone());
        }
        // `first_name` skips the `Vec<String>` allocation that
        // `names().into_iter().next()` would have caused. The
        // dispatch hot path runs this on every chat reply.
        registry.first_name()
    }

    /// Instance convenience for the static helper. Mirrors the
    /// ladder `explicit > configured default > first registered`.
    pub fn resolve_agent_name_for(
        &self,
        explicit: Option<&str>,
    ) -> Option<String> {
        Self::resolve_agent_name(
            &self.agent_registry,
            self.default_agent_name.read().as_deref(),
            explicit,
        )
    }

    /// Default user_id for the single-tenant deployments. A real
    /// auth middleware would override this with the resolved
    /// `RequestUserId` extension; until then we mirror the
    /// [`synthia_session::manager::SERVER_DEFAULT_USER_ID`]
    /// constant so all sink / controller / log paths agree on
    /// one user_id without taking it as a parameter on every
    /// route handler.
    pub fn default_user_id(&self) -> &str {
        synthia_session::manager::SERVER_DEFAULT_USER_ID
    }

    /// Snapshot the in-process usage counters for `/api/v1/chat/usage`.
    pub fn usage_metrics(&self) -> &UsageMetrics {
        &self.usage_metrics
    }
}

impl AppState {
    pub async fn new(
        workspace_root: PathBuf,
        config_path: Option<&std::path::PathBuf>,
    ) -> Result<Arc<Self>, String> {
        let workspace_config = load_workspace_config(&workspace_root, config_path)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to load workspace config, using env fallback");
                WorkspaceConfig::from_env()
            });

        let default_model = workspace_config.default_model.clone();

        let default_provider: Arc<dyn ModelProvider> = workspace_config
            .create_default_provider()
            .map(Arc::from)
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to create default provider");
                format!(
                    "No LLM provider available: {e}. Configure .agents/config.toml or set \
                     OPENAI_API_KEY/ANTHROPIC_API_KEY environment variables."
                )
            })?;

        let session_manager =
            Arc::new(SessionRegistry::new(workspace_root.join("sessions")));
        // Build the default built-in tool surface
        // (read/write/web_fetch/shell/TodoWrite), then register
        // the agent-facing `skill` tool on top. The skill
        // implementation lives in `synthia-skill` so the HTTP
        // list route and the LLM-facing tool share one home;
        // registering before wrapping in `RwLock` lets the
        // synchronous `register_skill_tool(&registry)` call
        // take `&ToolRegistry` directly without going through
        // an async lock guard.
        let tool_registry_built =
            build_default_tool_registry(workspace_root.clone());
        register_skill_tool(&tool_registry_built);
        let tool_registry = Arc::new(RwLock::new(tool_registry_built));

        let auth_config = Arc::new(load_auth_config(&workspace_root));
        let cors_config = Arc::new(load_cors_config(&workspace_root));
        let default_agent_name_value = load_default_agent(&workspace_root);

        // Default system prompt. The ReAct loop builds its
        // own system prompt per-dispatch via
        // `synthia_agent::prompt::PromptContext::assemble`, so
        // this is only the legacy/default fallback for
        // callers that read `AppState::system_prompt`
        // directly without an explicit descriptor.
        let system_prompt =
            synthia_agent::agent::re_act::DEFAULT_SYSTEM_PROMPT.to_string();

        // Seed the workflow skills on disk so the v1 HTTP
        // `GET /api/v1/skills` endpoint — which scans the workspace's
        // `.agents/skills/` directory — has visible data on a fresh
        // checkout. Existing user-installed skills are never
        // overwritten.
        if let Err(err) = seed_default_skills(&workspace_root) {
            tracing::warn!(error = %err, "failed to seed default skills");
        }

        // Build the multi-agent registry with the canonical
        // ReAct agent as the default member. Other panel
        // members (Judge / Critic / RedTeam / panel-specific
        // Proposers) can be registered later via
        // `AppState::agent_registry.put(...)`; they will
        // then appear in every agent's system prompt as handoff
        // targets.
        let agent_registry = Arc::new(AgentRegistry::new());
        if let Err(e) = agent_registry
            .put(AgentEntry::new(Arc::new(synthia_agent::ReActAgent::new(
                Arc::clone(&default_provider),
                {
                    let g = tool_registry.try_read();
                    match g {
                        Ok(g) => Arc::new(g.clone()),
                        Err(_) => Arc::new(ToolRegistry::new()),
                    }
                },
            ))))
            .await
        {
            tracing::warn!(
                error = %e,
                "failed to register canonical ReAct agent in AgentRegistry"
            );
        }

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

        Ok(Arc::new_cyclic(|_weak| Self {
            tool_registry,
            agent_registry,
            session_manager,
            workspace_root,
            default_model,
            workspace_config,
            default_provider,
            auth_config,
            cors_config,
            system_prompt,
            prompt_context,
            active_sessions: Arc::new(DashMap::new()),
            default_agent_name: parking_lot::RwLock::new(
                default_agent_name_value,
            ),
            usage_metrics: UsageMetrics::default(),
        }))
    }

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
            Arc::new(test_support::FakeProvider::new(vec![]));

        let tool_registry_built =
            build_default_tool_registry(workspace_root.clone());
        // Mirror the production wiring: the skill tool is part
        // of the default tool surface, not an opt-in extension.
        register_skill_tool(&tool_registry_built);
        let tool_registry = Arc::new(RwLock::new(tool_registry_built));

        let auth_config = Arc::new(load_auth_config(&workspace_root));
        let cors_config = Arc::new(load_cors_config(&workspace_root));

        let agent_registry = Arc::new(AgentRegistry::new());
        // Test state: register the canonical ReAct descriptor
        // (without holding a live provider) so the registry has
        // at least one entry and the peer-agent manifest snapshot
        // excludes it via the `self_name` filter.
        let _ = agent_registry
            .put(AgentEntry::new(Arc::new(synthia_agent::ReActAgent::new(
                Arc::clone(&default_provider),
                Arc::new(ToolRegistry::new()),
            ))))
            .await;

        let prompt_context = Arc::new(
            build_prompt_context(&workspace_root, &agent_registry, "agent")
                .await,
        );

        Self {
            tool_registry,
            agent_registry,
            session_manager: Arc::new(session_manager),
            workspace_root: workspace_root.clone(),
            default_model,
            workspace_config,
            default_provider,
            auth_config,
            cors_config,
            system_prompt: synthia_agent::agent::re_act::DEFAULT_SYSTEM_PROMPT
                .to_string(),
            prompt_context,
            active_sessions: Arc::new(DashMap::new()),
            default_agent_name: parking_lot::RwLock::new(None),
            usage_metrics: UsageMetrics::default(),
        }
    }

    /// Evaluate the readiness sub-checks backing the `/readyz` probe.
    ///
    /// Each tuple is `(check_name, passed)`. The server reports
    /// ready only when every check passes. Checks are cheap
    /// in-process reads — no network round-trips — so a probe
    /// firing once per second cannot pile up latency:
    /// - `chat_service`: the chat surface (the sole agent
    ///   interaction surface) is initialized. `create_router`
    ///   awaits it eagerly before the listener binds, so a
    ///   failure here means the router was built through a
    ///   non-standard path.
    /// - `agent_registry`: at least one agent is registered,
    ///   so dispatch has something to resolve.
    pub fn readiness_checks(&self) -> Vec<(&'static str, bool)> {
        vec![("agent_registry", self.agent_registry.first_name().is_some())]
    }

    /// Gets or creates a [`SessionController`] for `(user_id, session_id)`,
    /// restoring the session from the on-disk store if it is not already
    /// loaded in memory.
    pub async fn get_or_create_session_controller(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> synthia_core::Result<Arc<SessionController>> {
        self.get_or_create_session_controller_with_parent(
            user_id, session_id, None, None,
        )
        .await
    }

    /// Gets or creates a [`SessionController`] for `(user_id, session_id)`
    /// with an optional parent spawn depth.
    ///
    /// When `parent_depth` is `Some(d)`, the controller is created
    /// under a derived depth so future nested spawns could enforce
    /// `max_depth`. With subagent factories removed, the depth is
    /// currently informational; the controller is always treated as
    /// a root session at runtime.
    pub async fn get_or_create_session_controller_with_parent(
        &self,
        user_id: &str,
        session_id: &str,
        _parent_event_sender: Option<tokio::sync::mpsc::Sender<AgentEvent>>,
        _parent_depth: Option<usize>,
    ) -> synthia_core::Result<Arc<SessionController>> {
        let key = (user_id.to_string(), session_id.to_string());
        if let Some(entry) = self.active_sessions.get(&key) {
            return Ok(entry.clone());
        }

        if self.session_manager.get(session_id).await.is_none() {
            // client may supply a fresh task_id without a
            // pre-existing session. Create one eagerly so we never
            // have to round-trip through the metadata restore path.
            self.session_manager
                .create_with_user(session_id.to_string(), user_id.to_string())
                .await
                .map_err(|e| {
                    // 404 semantics (the requested session could
                    // not be established). The underlying cause
                    // rides along in the message — it never
                    // crosses the wire as structured data.
                    synthia_core::Error::not_found(format!(
                        "session '{session_id}' (create failed: {e})"
                    ))
                })?;
        }

        let queue = self.session_manager.input_queue();
        // After the panel/session refactor persistence is owned
        // by the `SessionSink` directly. The previous code
        // routed through `store().session_dir()` to locate the
        // JSONL file; the new path is `session_manager.sink()`
        // which yields a ready-to-use `Arc<dyn SessionSink>`
        // rooted at the same directory layout. The controller
        // no longer needs a separate `session_path` — the sink
        // owns the path.
        let session_sink = self.session_manager.sink(user_id, session_id);
        let deps = RunDependencies::new(
            Arc::clone(&self.default_provider),
            Arc::clone(&self.tool_registry),
            self.workspace_root.clone(),
            self.system_prompt.clone(),
        )
        .with_prompt_context(self.prompt_context.clone())
        .with_agent_registry(
            Arc::clone(&self.agent_registry),
            Arc::new(parking_lot::RwLock::new(
                self.default_agent_name.read().clone(),
            )),
        );

        let controller = SessionController::spawn(
            user_id,
            session_id,
            queue,
            session_sink,
            deps,
            crate::session::controller::DEFAULT_IDLE_TIMEOUT,
            Arc::new(AgentRunStreamFactory),
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
    load_server_config(workspace_root)
        .map(|c| c.auth)
        .unwrap_or_default()
}

/// Load the configured default agent name, if any.
///
/// Same lookup order as [`load_auth_config`]. Returns `None`
/// when no config is found or the field is unset.
fn load_default_agent(workspace_root: &std::path::Path) -> Option<String> {
    load_server_config(workspace_root).and_then(|c| c.default_agent)
}

/// Internal helper that loads the [`ServerConfig`] from disk
/// and returns it whole so callers can pick whichever field
/// they need. Returns `None` when no config file exists or all
/// candidates failed to parse.
fn load_server_config(
    workspace_root: &std::path::Path,
) -> Option<crate::config::ServerConfig> {
    use crate::config::ServerConfig;

    for candidate in [
        workspace_root.join("config.toml"),
        workspace_root.join(".synthia").join("config.toml"),
    ] {
        if candidate.exists() {
            match ServerConfig::load(&candidate) {
                Ok(cfg) => return Some(cfg),
                Err(e) => {
                    tracing::warn!(
                        path = %candidate.display(),
                        error = %e,
                        "failed to load server config; falling back to defaults"
                    );
                }
            }
        }
    }
    None
}

/// Build the prompt-context manifest the assembler consumes.
///
/// Skills are discovered via [`synthia_skill::discover_skills`],
/// which walks project + user roots and returns fully-resolved
/// [`synthia_skill::Skill`] values (mirrors opencode /
/// Anthropic Agent Skills convention). The same helper the
/// `skill` tool uses, so the prompt manifest and the runtime
/// always agree on what's available.
///
/// Peer agents are snapshotted from the live `AgentRegistry`
/// inline; the caller-supplied `self_name` is excluded so the
/// running agent never sees itself as a handoff target.
async fn build_prompt_context(
    workspace_root: &std::path::Path,
    agent_registry: &Arc<AgentRegistry>,
    self_name: &str,
) -> PromptContext {
    let mut ctx = PromptContext::default();

    // Skills: same discovery helper as the `skill` tool, so
    // the prompt manifest and the tool never disagree on
    // what's available. Project roots win on collisions
    // (opencode convention); missing descriptions fall back
    // to the first non-empty body line.
    for skill in synthia_skill::discover_skills(workspace_root) {
        ctx = ctx.with_skill(skill.name.clone(), skill.effective_description());
    }

    // Peer agents: snapshot the registry, exclude `self_name`,
    // feed each remaining descriptor into the builder. A
    // registry error or empty list is non-fatal: the manifest
    // simply carries no peer agents.
    if let Ok(entries) = agent_registry.list(None).await {
        for entry in entries.into_iter().filter(|e| e.name() != self_name) {
            ctx = ctx.with_agent(entry.descriptor());
        }
    }

    ctx
}

/// Load CORS configuration from workspace `config.toml` files.
///
/// Lookup order:
/// 1. `{workspace_root}/config.toml` (primary).
/// 2. `{workspace_root}/.synthia/config.toml` (alternate location).
/// 3. `CorsConfig::default()` (empty lists → permissive by default).
fn load_cors_config(workspace_root: &std::path::Path) -> CorsConfig {
    use crate::config::ServerConfig;

    for candidate in [
        workspace_root.join("config.toml"),
        workspace_root.join(".synthia").join("config.toml"),
    ] {
        if candidate.exists() {
            match ServerConfig::load(&candidate) {
                Ok(cfg) => return cfg.cors,
                Err(e) => {
                    tracing::warn!(
                        path = %candidate.display(),
                        error = %e,
                        "failed to load server config; using default CORS"
                    );
                }
            }
        }
    }
    CorsConfig::default()
}

#[cfg(test)]
mod tests {
    //! Unit tests for `resolve_agent_name`. The three-tier
    //! fallback ladder (`explicit > configured default >
    //! first registered`) is the single dispatch path every
    //! request — chat, scheduler — flows through, so a
    //! regression in the priority ordering would surface as
    //! silent "wrong agent" responses. We pin every branch
    //! here.
    use std::{pin::Pin, sync::Arc};

    use futures::stream;
    use synthia_agent::{
        Agent,
        AgentDescriptor,
        AgentEntry,
        AgentEvent,
        AgentInput,
        AgentRegistry,
    };
    use synthia_core::registry::{Registry, RegistryItem};
    use synthia_session::manager::SessionRegistry;
    use tokio_util::sync::CancellationToken;

    use super::AppState;

    struct StubAgent {
        desc: AgentDescriptor,
    }

    impl RegistryItem for StubAgent {
        fn name(&self) -> &str {
            &self.desc.name
        }

        fn description(&self) -> &str {
            &self.desc.description
        }
    }

    #[async_trait::async_trait]
    impl Agent for StubAgent {
        fn descriptor(&self) -> &AgentDescriptor {
            &self.desc
        }

        async fn run(
            &self,
            _input: AgentInput,
            _cancel: Arc<CancellationToken>,
        ) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send + 'static>>
        {
            Box::pin(stream::empty())
        }
    }

    fn stub_entry(name: &str) -> AgentEntry {
        AgentEntry::new(Arc::new(StubAgent {
            desc: AgentDescriptor {
                name: name.into(),
                description: format!("desc for {name}"),
                kind: "react".into(),
                version: "1.0.0".into(),
                instructions: String::new(),
                capabilities: vec!["tools".into()],
                tools: vec![],
                model_hint: None,
                handoffs: vec![],
                handoff_hint: None,
                output_schema: None,
                owner: None,
                domain: None,
                persona: None,
                display_name: None,
            },
        }))
    }

    #[tokio::test]
    async fn explicit_name_wins_over_default() {
        let reg = AgentRegistry::new();
        reg.put(stub_entry("a")).await.unwrap();
        reg.put(stub_entry("default-one")).await.unwrap();
        let got =
            AppState::resolve_agent_name(&reg, Some("default-one"), Some("a"));
        assert_eq!(got, Some("a".to_string()));
    }

    #[tokio::test]
    async fn explicit_name_missing_falls_back_to_default() {
        let reg = AgentRegistry::new();
        reg.put(stub_entry("default-one")).await.unwrap();
        let got = AppState::resolve_agent_name(
            &reg,
            Some("default-one"),
            Some("does-not-exist"),
        );
        assert_eq!(got, Some("default-one".to_string()));
    }

    #[tokio::test]
    async fn no_explicit_no_default_falls_back_to_first_registered() {
        let reg = AgentRegistry::new();
        reg.put(stub_entry("first")).await.unwrap();
        reg.put(stub_entry("second")).await.unwrap();
        let got = AppState::resolve_agent_name(&reg, None, None);
        // HashMap iteration order is unspecified — we
        // accept any registered name. The point is: not
        // None.
        assert!(got.is_some());
        assert!(reg.names().contains(&got.unwrap()));
    }

    #[tokio::test]
    async fn empty_registry_returns_none() {
        let reg = AgentRegistry::new();
        assert_eq!(AppState::resolve_agent_name(&reg, None, None), None);
        assert_eq!(
            AppState::resolve_agent_name(&reg, Some("default"), None),
            None
        );
        assert_eq!(
            AppState::resolve_agent_name(
                &reg,
                Some("default"),
                Some("explicit")
            ),
            None
        );
    }

    /// `explicit = Some("")` (empty string) MUST be treated
    /// the same as `None` by the caller (`extract_agent_name`
    /// the chat layer already converts "" to None), but
    /// defensively, if it ever leaks through, the registry
    /// lookup must also produce `None` rather than silently
    /// match a (non-existent) ""-named agent.
    #[tokio::test]
    async fn empty_explicit_string_does_not_match_a_registry_default() {
        let reg = AgentRegistry::new();
        reg.put(stub_entry("only-one")).await.unwrap();
        let got = AppState::resolve_agent_name(&reg, None, Some(""));
        // "" is not registered → fall back to first
        // registered. If the registry had an "" entry, this
        // test would still return Some(""), but we don't
        // register one.
        assert_eq!(got, Some("only-one".to_string()));
    }

    // ---------- build_prompt_context tests ----------
    //
    // `build_prompt_context` reads the
    // `<workspace>/.agents/skills/` directory and snapshots
    // the live agent registry. Tests assert on the
    // assembled prompt text — the public surface — instead
    // of poking at the manifest's internal fields (those
    // are `pub(crate)` and not visible from this crate).
    //
    //   1. Missing skills dir → no `<available_skills>` block.
    //   2. Skills are sorted alphabetically by name.
    //   3. Per-skill enable toggle filters out disabled
    //      skills from the prompt text.
    //   4. Tool schemas never appear in the assembled prompt.
    //   5. Frontmatter failure falls back to a placeholder.
    //   6. Valid frontmatter wins over body.
    //   7. Incomplete skill dirs (no `SKILL.md`) are skipped.

    fn write_skill_md(dir: &std::path::Path, name: &str, content: &str) {
        let skill_dir = dir.join(".agents").join("skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    fn empty_registry() -> std::sync::Arc<synthia_agent::AgentRegistry> {
        std::sync::Arc::new(synthia_agent::AgentRegistry::new())
    }

    fn descriptor() -> AgentDescriptor {
        AgentDescriptor {
            name: "agent".into(),
            description: "ReAct loop".into(),
            kind: "react".into(),
            version: "1.0.0".into(),
            instructions: "BASE".into(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            model_hint: None,
            handoffs: Vec::new(),
            handoff_hint: None,
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
        }
    }

    /// Process-global mutex that serialises every test
    /// touching `$HOME`. `discover_skills` reads user-level
    /// skills from `$HOME/{.claude,.agents}/skills`, and
    /// parallel tests would race each other: one mutates
    /// `$HOME`, another reads from a stale path. Cheap to
    /// hold because the locked section is short.
    static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Pin `$HOME` to a tempdir for the duration of a test.
    /// `synthia_skill::discover_skills` reads user-level
    /// skills from `$HOME/{.claude,.agents}/skills`, and the
    /// developer's machine typically has a populated
    /// `~/.claude/skills/`. Without scoping, every test would
    /// pick up user skills too — defeating the assertions.
    ///
    /// `home_dir` is exposed for tests that want to seed
    /// user-level skills; the field is read by those tests so
    /// it's not dead code.
    struct ScopedHome {
        // Fields are dropped in declaration order — so this
        // struct restores `$HOME` and tears down the tempdir
        // before the mutex is released. That ordering matters:
        // if the lock were released first, a sibling test
        // could observe `HOME` mid-restore and read from a
        // path that no longer exists.
        previous: Option<std::ffi::OsString>,
        home_dir: tempfile::TempDir,
        // Hold the global mutex for the guard's lifetime so
        // other tests cannot mutate `$HOME` while this guard
        // is alive.
        _guard: std::sync::MutexGuard<'static, ()>,
    }
    impl ScopedHome {
        /// Lock the global HOME mutex and pin `$HOME` to a
        /// fresh tempdir. The returned guard's `Drop` releases
        /// the lock and restores the previous `$HOME`.
        fn new() -> Self {
            // Ignore poisoned mutex: if a sibling test
            // panicked mid-test we still want subsequent
            // tests to run (env restoration is idempotent).
            let guard = HOME_LOCK
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let home_dir = tempfile::tempdir().unwrap();
            let previous = std::env::var_os("HOME");
            unsafe {
                std::env::set_var("HOME", home_dir.path());
            }
            Self {
                previous,
                home_dir,
                _guard: guard,
            }
        }
    }
    impl Drop for ScopedHome {
        fn drop(&mut self) {
            unsafe {
                match self.previous.take() {
                    Some(v) => std::env::set_var("HOME", v),
                    None => {
                        std::env::remove_var("HOME");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn build_prompt_context_with_no_skills_dir_omits_skills_block() {
        let _home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();
        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        assert!(!out.contains("<available_skills>"));
        assert!(!out.contains("<available_agents>"));
        assert!(out.contains("<identity>"));
    }

    #[tokio::test]
    async fn build_prompt_context_sorts_skills_alphabetically() {
        let _home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();
        // Insert in REVERSE alphabetical order.
        write_skill_md(
            dir.path(),
            "zoo",
            "---\nname: zoo\ndescription: Z\n---\n\nbody\n",
        );
        write_skill_md(
            dir.path(),
            "apple",
            "---\nname: apple\ndescription: A\n---\n\nbody\n",
        );
        write_skill_md(
            dir.path(),
            "mango",
            "---\nname: mango\ndescription: M\n---\n\nbody\n",
        );
        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        // Alphabetical order ⇒ `<name>apple</name>` before
        // `<name>mango</name>` before `<name>zoo</name>` in
        // the assembled prompt text.
        let apple = out.find("<name>apple</name>").expect("apple in prompt");
        let mango = out.find("<name>mango</name>").expect("mango in prompt");
        let zoo = out.find("<name>zoo</name>").expect("zoo in prompt");
        assert!(apple < mango && mango < zoo);
    }

    #[tokio::test]
    async fn build_prompt_context_drops_tools_from_assembled_text() {
        let _home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();
        // Tools are deliberately NOT carried by the prompt
        // context — they travel on the completion-request
        // `tools` channel. The assembled prompt therefore
        // has no `<available_tools>` block and no
        // "Use only these tools:" grounding line.
        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        assert!(!out.contains("<available_tools>"));
        assert!(!out.contains("Use only these tools"));
    }

    /// `discover_skills` follows the opencode / Anthropic
    /// convention: malformed SKILL.md files are silently
    /// dropped, not placeheld. A broken skill MUST NOT
    /// poison the prompt manifest or surface as a `<name>`
    /// entry that the model would try to invoke.
    #[tokio::test]
    async fn build_prompt_context_drops_malformed_skill_md() {
        let _home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(dir.path(), "broken", "no delimiters\n");
        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        assert!(
            !out.contains("<name>broken</name>"),
            "malformed skill must be dropped, not advertised; got:\n{out}"
        );
    }

    #[tokio::test]
    async fn build_prompt_context_with_valid_frontmatter_uses_description() {
        let _home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(
            dir.path(),
            "good",
            "---\nname: good\ndescription: Use this for X\n---\n\nbody\n",
        );
        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        assert!(
            out.contains("<description>Use this for X</description>"),
            "description must surface in the XML envelope; got:\n{out}"
        );
    }

    /// Anthropic Agent Skills makes `description` optional —
    /// the loader falls back to the first non-empty body
    /// line, and the prompt manifest uses the same fallback.
    /// Pin the contract so a regression that hard-fails
    /// skills-without-description breaks loudly.
    #[tokio::test]
    async fn build_prompt_context_falls_back_to_body_when_description_missing()
    {
        let _home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(
            dir.path(),
            "nodoc",
            "---\nname: nodoc\n---\n\n# No doc skill\n\nBody.\n",
        );
        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        assert!(out.contains("<name>nodoc</name>"));
        assert!(
            out.contains("<description>No doc skill</description>"),
            "missing description must fall back to first body line; \
             got:\n{out}"
        );
    }

    #[tokio::test]
    async fn build_prompt_context_skips_dirs_without_skill_md() {
        let _home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();
        // Create a directory that has no SKILL.md
        // (e.g. a partial checkout).
        std::fs::create_dir_all(
            dir.path().join(".agents").join("skills").join("incomplete"),
        )
        .unwrap();
        // Create a valid one.
        write_skill_md(
            dir.path(),
            "valid",
            "---\nname: valid\ndescription: V\n---\n\nbody\n",
        );
        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        assert!(out.contains("<name>valid</name>"));
        assert!(!out.contains("<name>incomplete</name>"));
    }

    /// User-level skills at `$HOME/.claude/skills/` MUST
    /// surface in the prompt manifest (Anthropic / OpenCode
    /// convention). Project skills win on name collisions.
    #[tokio::test]
    async fn build_prompt_context_includes_user_skills_from_home() {
        let home = ScopedHome::new();
        let dir = tempfile::tempdir().unwrap();

        let user_skill_dir = home
            .home_dir
            .path()
            .join(".claude")
            .join("skills")
            .join("user-only");
        std::fs::create_dir_all(&user_skill_dir).unwrap();
        std::fs::write(
            user_skill_dir.join("SKILL.md"),
            "---\nname: user-only\ndescription: From ~/.claude.\n---\n\nBody.\n",
        )
        .unwrap();

        write_skill_md(
            dir.path(),
            "project-only",
            "---\nname: project-only\ndescription: Project.\n---\n\nBody.\n",
        );

        let pc =
            super::build_prompt_context(dir.path(), &empty_registry(), "agent")
                .await;
        let out = pc.assemble(&descriptor());
        assert!(
            out.contains("<name>user-only</name>"),
            "user-level skill must surface; got:\n{out}"
        );
        assert!(
            out.contains("<name>project-only</name>"),
            "project skill must surface; got:\n{out}"
        );
    }

    /// `for_test`'s tool registry MUST include the agent-facing
    /// `skill` tool so the test surface mirrors the production
    /// wiring. If the registration ever silently regresses (e.g.
    /// the call gets moved off the `build_default_tool_registry`
    /// path), this test fails loudly instead of letting the LLM
    /// silently lose access to skill workflows during route-level
    /// testing.
    #[tokio::test]
    async fn for_test_tool_registry_includes_skill_tool() {
        let dir = tempfile::tempdir().unwrap();
        let session_registry =
            SessionRegistry::new(dir.path().join("sessions"));
        let state =
            AppState::for_test(session_registry, dir.path().to_path_buf())
                .await;
        let reg = state.tool_registry.read().await;
        let entry = reg
            .get("skill")
            .await
            .expect("skill lookup must succeed")
            .expect("skill tool must be registered in the default registry");
        assert_eq!(entry.tool_instance().name(), "skill");
    }
}
