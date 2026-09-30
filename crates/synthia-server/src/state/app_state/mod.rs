//! `AppState`: top-level shared state container for the server.
//!
//! ## Layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `boot` | `AppState::new` / `with_server_config`: resolve configs, build the registry, wire every field. |
//! | `for_test` | The `test-utils` constructor with in-memory components and a fake provider. |
//! | `providers` | `ProviderCatalogue`: every configured provider, and the resolution a turn's `model` selection goes through. |
//! | `sessions` | `get_or_create_session_controller`: the per-`(user, session)` controller cache. |
//! | `usage` | `UsageMetrics` counters + `UsageSnapshot`. |
//! | `tools` | The default plugin tool surface + the `AppliedToolSurface` record. |
//! | `schedules` | `ScheduleStore` + `Scheduler` for the `/api/v1/schedules/*` HTTP surface. |
//! | `workflows` | In-memory `WorkflowSpec` registry + plan validation for `/api/v1/workflows/*`. |
//! | `evals` | In-memory `EvalSuite` registry + keyword-metric runner for `/api/v1/evals/*`. |

use std::{path::PathBuf, sync::Arc};

use synthia::{
    harness::AgentRegistry,
    provider::{config::WorkspaceConfig, traits::ModelProvider},
    session::{SharedSessionSearch, manager::SessionRegistry},
    tool::ToolRegistry,
};
use tokio::sync::RwLock;

use crate::config::{AuthConfig, CorsConfig};

mod boot;
mod evals;
mod providers;
mod schedules;
pub(crate) mod self_mcp;
mod sessions;
mod tools;
mod usage;
mod workflows;

#[cfg(any(test, feature = "test-utils"))]
mod for_test;

pub use evals::EvalsState;
pub use providers::{ModelSelectionError, ProviderCatalogue};
pub use schedules::SchedulesState;
pub use tools::AppliedToolSurface;
#[cfg(test)]
pub(crate) use tools::plugin_tool_registry;
pub use usage::{UsageMetrics, UsageSnapshot};
pub use workflows::WorkflowsState;

pub struct AppState {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Multi-agent registry. Populated at startup with the
    /// canonical [`ReActAgent`](synthia::harness::ReActAgent) descriptor so any panel/judge
    /// agents registered later surface in the system prompt as
    /// handoff targets. The `self` agent is excluded from its
    pub agent_registry: Arc<AgentRegistry>,
    pub session_search: SharedSessionSearch,
    pub session_manager: Arc<SessionRegistry>,
    pub workspace_root: PathBuf,
    pub default_model: String,
    pub workspace_config: WorkspaceConfig,
    pub default_provider: Arc<dyn ModelProvider>,
    /// Every provider this deployment configures, keyed by its
    /// `[providers.<name>]` entry name — the map a turn's `model`
    /// selection resolves against.
    ///
    /// [`Self::default_provider`] is `catalogue.default_handle()`:
    /// the handle for `workspace_config.default_provider`. It is kept
    /// as its own field because it is the fallback every caller
    /// without an override uses, and because a fixture may replace it
    /// with a fake; the catalogue is what makes the *other* providers
    /// selectable at all.
    pub provider_catalogue: Arc<ProviderCatalogue>,
    /// Authentication configuration shared with the auth middleware.
    /// `AuthLayer` reads `api_keys` / `key_to_user` from this and
    /// surfaces a `RequestUserId` extension on every request.
    pub auth_config: Arc<AuthConfig>,
    /// CORS configuration shared with the router.
    /// Read by `build_cors_layer` to attach a `CorsLayer` to the
    /// `Router` so browser clients (e.g. synthia-web on
    /// http://localhost:5173) can call the API endpoints.
    pub cors_config: Arc<CorsConfig>,
    /// R29: gate for the `POST .../operation` endpoint. Read by
    /// `create_router` to decide whether to register the route.
    pub operations_config: Arc<crate::config::OperationEndpointConfig>,
    /// Active session controllers keyed by `(user_id, session_id)`.
    ///
    /// Per-session event broadcasters live on the controller
    /// itself; SSE/WebSocket subscribers obtain a
    /// `tokio::sync::broadcast::Receiver` via
    /// [`SessionController::subscribe`](crate::session::controller::SessionController::subscribe).
    pub active_sessions: Arc<crate::state::ActiveSessions>,
    /// Sub-agent session router: gives every delegated child a
    /// first-class session of its own (see
    /// `crate::session::subagent_router`). Installed once at boot
    /// over the same registry + controller cache the HTTP surface
    /// uses, and threaded into every session controller's
    /// [`RunDependencies`](crate::session::controller::RunDependencies).
    pub subagent_router: Arc<crate::session::subagent_router::SubagentRouter>,
    /// Pre-rendered default system prompt. The agent loop
    /// currently builds its own prompt from the descriptor
    /// via [`synthia::harness::PromptContext::assemble`],
    /// so this field exists as a lock-free default for
    /// legacy callers that read it directly. Stored on
    /// `AppState` so the read is cheap on every dispatch.
    pub system_prompt: String,
    /// Pre-built prompt manifest (skills + peer agents + tool
    /// definitions) consumed by the
    /// [`synthia::harness::PromptContext::assemble`]
    /// method. Built once at startup and shared via `Arc` into
    /// every [`crate::session::controller::RunDependencies`] —
    /// the manifest never mutates after boot, so wrapping it
    /// in an `Arc` lets every dispatch and every per-agent
    /// `build_react_agent` call share the same allocation
    /// instead of deep-cloning the full skills + peer-agent
    /// list on each one.
    pub prompt_context: Arc<synthia::harness::PromptContext>,
    /// Server-wide steering policy installed on every agent the
    /// server builds: loop detection, tool budget, catastrophic
    /// shell deny, workspace boundary, prompt injection guards;
    /// context-budget / iteration / truncation hints; adaptive
    /// concurrency; the debug logging hook. Built once at boot
    /// via [`synthia::steering::Steering::default_policy`] with
    /// the workspace root and shared by reference.
    pub steering: Arc<synthia::steering::Steering>,
    /// Configured default agent name. Loaded from
    /// `config.agent.default` at startup. `None` falls back to
    /// the first agent registered in [`Self::agent_registry`].
    ///
    /// Backed by [`parking_lot::RwLock`] (not
    /// [`tokio::sync::RwLock`]) so the run factory can resolve
    /// the default agent name synchronously without awaiting.
    pub default_agent_name: parking_lot::RwLock<Option<String>>,
    /// Default per-run iteration cap applied to every dispatch
    /// unless a per-run override is supplied. Loaded from
    /// `agents.<name>.max_steps` in the server config; falls
    /// back to
    /// [`synthia::harness::DEFAULT_MAX_ITERATIONS`]
    /// when unset. The agent field's own `[1, 4096]` clamp is
    /// honoured here too.
    pub default_max_iterations: usize,
    /// R16: optional compaction policy resolved from
    /// `agents.<name>.compaction` at boot. Forwarded to every
    /// session controller's `RunDependencies`.
    pub default_compaction: Option<synthia::context::CompactionSettings>,
    /// The deployment configuration the boot resolved (R53/R54), kept
    /// whole so the operator-facing surfaces can answer "what was this
    /// process started with?" without re-reading the file — and so
    /// `max_agents` and the bind address have one source.
    pub server_config: Option<crate::config::ServerConfig>,
    /// R58: the agent's allow/deny list resolved from
    /// `agents.<default>.{allowed_tools,denied_tools}` at boot.
    /// Forwarded to every run and installed on the registered agents.
    pub default_tool_restriction: Option<Arc<synthia::tool::ToolRestriction>>,
    /// R50: the reasoning loop resolved from
    /// `agents.<default>.strategy` at boot. Forwarded to every
    /// session controller's `RunDependencies` and installed on the
    /// registered agents, so a session run and a registered agent use
    /// the same loop. `None` keeps ReAct.
    pub default_strategy:
        Option<Arc<dyn synthia::harness::agent::ReasoningStrategy>>,
    /// R21: live MCP clients for the configured servers. Held for
    /// the process lifetime so their child processes stay alive
    /// (`StdioTransport` kills its child on drop) and so a
    /// consumer can address the servers by name.
    pub mcp_clients: Vec<(String, Arc<synthia::mcp::McpClient>)>,
    /// R30: per-server MCP health captured by the boot sweep —
    /// one entry per configured server, healthy or not. Lets a
    /// consumer (readiness probe, operator endpoint) see which
    /// integrations are live without re-reading the config.
    pub mcp_health: Vec<synthia::mcp::ServerHealth>,
    /// R34: the effective tool surface after the `[tools]` config
    /// section was applied to the registry at boot. Unknown config
    /// names were dropped with a warning (see
    /// `AppliedToolSurface::skipped`), so this is what an operator
    /// can trust was actually written. The run factory installs
    /// `AppliedToolSurface::policy` on every agent it builds.
    pub tool_surface: AppliedToolSurface,
    /// R34: per-state cache for the `GET /api/v1/tools` projection,
    /// keyed on [`ToolRegistry::version`]. Per-state rather than a
    /// process-wide static so two servers in one process cannot
    /// serve each other's list; every registry mutation that changes
    /// the model-facing list bumps the version, so a hit is only ever
    /// the same registry at the same version.
    pub tool_defs_cache: parking_lot::RwLock<
        Option<(u64, Arc<Vec<synthia::provider::ToolDefinition>>)>,
    >,
    /// Process-wide usage counters surfaced by
    /// `GET /api/v1/chat/usage`.
    ///
    /// Shared with **every** session controller this state spawns
    /// (installed on their [`RunDependencies`] and, from there, on
    /// the controller's event funnel — the single write site, which
    /// records each run's `SystemEvent::Usage` totals and counts the
    /// run on `SystemEvent::SessionEnded`). Read-only via
    /// [`AppState::usage_metrics`]; the `Arc` is what lets the
    /// counters outlive any one controller, so tokens spent by a
    /// closed session still show up in the process totals.
    ///
    /// [`RunDependencies`]: crate::session::controller::RunDependencies
    pub usage_metrics: Arc<UsageMetrics>,
    /// Multimodal attachment store (R11). Backs the
    /// `/api/v1/attachments` REST surface and lets the agent
    /// runtime fetch the inline base64 for a stored image
    /// without round-tripping the original uploader.
    pub attachment_store: Arc<synthia::attachment::AttachmentStore>,
    /// Long-term memory tier (files under the workspace's
    /// `.synthia/memory*` scopes). The same handle the search
    /// service's memory domain recalls from.
    pub memory: Arc<dyn synthia::context::Memory>,
    /// The cross-domain search engine: one `synthia_search`
    /// registry over tool / mcp / skill / memory / agent / session,
    /// registered as the model-facing `search` tool (deferred
    /// exposure — capability 1) and projected by
    /// `GET /api/v1/search` (capability 2).
    pub search: Arc<crate::search::SearchService>,
    /// Schedules subsystem — disk-backed `ScheduleStore` plus a
    /// runtime-neutral `Scheduler`. Backs
    /// `/api/v1/schedules/{list,get,create,patch,delete,tick}`.
    pub schedules: SchedulesState,
    /// Workflows subsystem — in-memory `WorkflowSpec` registry
    /// plus plan validation. Backs `/api/v1/workflows/*`. Real
    /// execution lands in a follow-up turn (the runtime needs a
    /// `WorkflowHost` that calls back into the agent harness).
    pub workflows: WorkflowsState,
    /// Evals subsystem — in-memory `EvalSuite` registry plus a
    /// keyword-only runner. Backs `/api/v1/evals/*`. LLM-judge /
    /// schema-validation metrics land with a real `EvalAgent`.
    pub evals: EvalsState,
    /// The process's single wall-clock source. Every component that
    /// stamps a timestamp (the session log, operation snapshots, the
    /// attachment store, the MCP supervisor's tick) reads it from
    /// here instead of calling `chrono::Utc::now()` on its own, so a
    /// test can pin the whole deployment's clock in one place.
    pub clock: synthia::core::SharedClock,
}

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

    /// Resolve a turn's `model` selection to the provider that runs
    /// it.
    ///
    /// Accepted spellings, on every surface that carries a model
    /// selection:
    ///
    /// - `None` / blank — the deployment's default provider;
    /// - `"<provider>/<model>"` — the shape `/api/v1/models`
    ///   advertises per entry, and what the chat UI posts. The split
    ///   is at the *first* slash, so a model id that contains one
    ///   (`meta-llama/llama-3.3-70b-instruct`) still resolves;
    /// - a bare `"<model>"` that exactly one configured provider
    ///   advertises — the Anthropic protocol's `model` field is a
    ///   bare model id, so `/v1/messages` needs this spelling.
    ///
    /// A value that resolves to nothing is an error naming it: an
    /// override must never silently run the default, or the user's
    /// model selection is a no-op they cannot see.
    ///
    /// [`ProviderCatalogue::resolve`] documents how each mismatch is
    /// reported and [`ModelSelectionError::status`] the HTTP status
    /// it maps to.
    pub(crate) fn resolve_provider(
        &self,
        selection: Option<&str>,
    ) -> Result<Arc<dyn ModelProvider>, ModelSelectionError> {
        self.provider_catalogue.resolve(selection)
    }

    /// Default user_id for the single-tenant deployments. A real
    /// auth middleware would override this with the resolved
    /// `RequestUserId` extension; until then we mirror the
    /// [`synthia::session::manager::SERVER_DEFAULT_USER_ID`]
    /// constant so all sink / controller / log paths agree on
    /// one user_id without taking it as a parameter on every
    /// route handler.
    pub fn default_user_id(&self) -> &str {
        synthia::session::manager::SERVER_DEFAULT_USER_ID
    }

    /// Snapshot the in-process usage counters for `/api/v1/chat/usage`.
    pub fn usage_metrics(&self) -> &UsageMetrics {
        self.usage_metrics.as_ref()
    }

    /// How many agents this deployment may hold
    /// (`max_agents` in the deployment config, default
    /// [`crate::config::DEFAULT_MAX_AGENTS`]).
    ///
    /// Enforced by `POST /api/v1/agents`: a public deployment must not be
    /// able to grow its agent registry without bound.
    pub fn max_agents(&self) -> usize {
        self.server_config
            .as_ref()
            .map(|config| config.max_agents)
            .unwrap_or(crate::config::DEFAULT_MAX_AGENTS)
    }
}

impl AppState {
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
}
