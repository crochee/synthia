//! The boot-time helpers the server wires together:
//! readers for the agent table (`load_default_*`), the MCP
//! server registration sweep, the `[tools]` surface application,
//! and the prompt manifest builder. Each is a single-purpose
//! step `AppState::with_server_config` orchestrates; the file
//! is intentionally a flat list of free functions (no
//! shared state, no cross-calls except the private
//! helpers `apply_tools_config`
//! and `register_configured_mcp_servers` use).

use std::sync::Arc;

use synthia::{
    core::registry::{Registry as _, RegistryItem},
    harness::{AgentRegistry, PromptContext},
    tool::ToolRegistry,
};

// ---------------------------------------------------------------------------
// Agent table readers (R16 / R50 / R55 / R58)
// ---------------------------------------------------------------------------

/// Resolve the configured default iteration cap.
///
/// Reads `agents.<name>.max_steps` from the server config (the
/// same `[agents.<name>]` block that sets `description`,
/// `model`, etc.) and clamps it into `[1, 4096]`. Returns
/// `None` when the field is unset so callers can fall back to
/// [`synthia::harness::DEFAULT_MAX_ITERATIONS`].
pub(crate) fn load_default_max_iterations(
    cfg: Option<&crate::config::ServerConfig>,
    default_agent_name: Option<&str>,
) -> Option<usize> {
    let name = default_agent_name?;
    let raw = cfg?.agents.get(name)?.max_steps?;
    // enforces, so the operator can express "no cap" (u32::MAX)
    // without enabling an unbounded session.
    Some(raw.clamp(1, 4096) as usize)
}

/// R16: reads `agents.<name>.compaction` from the server config
/// and returns it for the run factory. `None` when unset — the
/// loop then keeps its default context manager.
pub(crate) fn load_default_compaction(
    cfg: Option<&crate::config::ServerConfig>,
    default_agent_name: Option<&str>,
) -> Option<synthia::context::CompactionSettings> {
    let name = default_agent_name?;
    cfg?.agents.get(name)?.compaction
}

/// `agents.<name>` keys the boot parses but does not apply.
///
/// R55: `[agents]` grew a display/feature shape (description, colour,
/// per-agent tool lists) whose consumers were never built — the listing
/// serves registry descriptors, the model comes from the descriptor's
/// `model_hint`, and what a run may *see and call* comes from the
/// `[tools]` surface plus the registry's privacy flag. A key that
/// parses and does nothing is worse than an absent one: an operator who
/// wrote `denied_tools = ["shell"]` has restricted nothing and has no
/// way to find out.
///
/// R124: `sandbox` (R62) is parsed and exposed via
/// [`crate::config::AgentConfig::sandbox_policy_name`], yet the run
/// factory's boot-time `ShellTool::new()` (registered at
/// `state/app_state/tools.rs`) is what every run actually uses; the
/// per-agent override is therefore silently inert today. Classify it
/// as unapplied so the boot log names the gap — the actual consumer
/// (`with_policy` re-bind in the run factory) is tracked in
/// `docs/research/synthia-r124-holistic-design.md` §2 as a follow-up.
///
/// Returned rather than logged so the classification is testable; the
/// caller logs one line per configured agent.
pub(crate) fn unapplied_agent_keys(
    agent: &crate::config::AgentConfig,
) -> Vec<&'static str> {
    let mut keys = Vec::new();
    if agent.description.is_some() {
        keys.push("description");
    }
    if agent.model.is_some() {
        keys.push("model");
    }
    if agent.hidden {
        keys.push("hidden");
    }
    if agent.color.is_some() {
        keys.push("color");
    }
    if agent.sandbox_policy_name().is_some() {
        keys.push("sandbox");
    }
    keys
}

/// Log every configured agent that carries keys the boot ignores.
pub(crate) fn warn_unapplied_agent_keys(config: &crate::config::ServerConfig) {
    for (name, agent) in &config.agents {
        let unapplied = unapplied_agent_keys(agent);
        if unapplied.is_empty() {
            continue;
        }
        tracing::warn!(
            target: "synthia.agent",
            agent = %name,
            keys = ?unapplied,
            "agent config keys are parsed but not applied; what a run may \
             see and call comes from [tools] (groups / hidden / deferred) \
             and the registry's privacy flag — see DEPLOYMENT.md"
        );
    }
}

/// R58: reads `agents.<name>.{allowed_tools,denied_tools}` and turns
/// them into the [`synthia::tool::ToolRestriction`] the run factory
/// installs.
///
/// `allowed_tools` becomes the allow-list (empty means "no allow-list",
/// not "nothing is allowed"), `denied_tools` the deny-list; both empty
/// means no restriction at all, which is the common case and stays
/// allocation-free.
pub(crate) fn load_default_tool_restriction(
    cfg: Option<&crate::config::ServerConfig>,
    default_agent_name: Option<&str>,
) -> Option<Arc<synthia::tool::ToolRestriction>> {
    let name = default_agent_name?;
    let agent = cfg?.agents.get(name)?;
    if agent.allowed_tools.is_empty() && agent.denied_tools.is_empty() {
        return None;
    }
    let allow =
        (!agent.allowed_tools.is_empty()).then(|| agent.allowed_tools.clone());
    tracing::info!(
        agent = %name,
        allow = ?allow,
        deny = ?agent.denied_tools,
        "agent tool restriction configured"
    );
    Some(Arc::new(synthia::tool::ToolRestriction {
        allow,
        deny: agent.denied_tools.clone(),
    }))
}

/// R50: reads `agents.<name>.strategy` and resolves it to a strategy
/// instance for the run factory and the registered agents.
///
/// `None` when unset (ReAct), and also when the name is not one this
/// build ships: the operator's typo is logged at `warn` and the agent
/// falls back rather than the server refusing to start — the same
/// fail-soft contract `[tools]` and `[mcp_servers]` follow.
pub(crate) fn load_default_strategy(
    cfg: Option<&crate::config::ServerConfig>,
    default_agent_name: Option<&str>,
) -> Option<Arc<dyn synthia::harness::agent::ReasoningStrategy>> {
    let name = default_agent_name?;
    let configured = cfg?.agents.get(name)?.strategy.as_deref()?;
    match synthia::harness::agent::from_name(configured) {
        Ok(strategy) => {
            tracing::info!(
                target: "synthia.agent",
                agent = %name,
                strategy = strategy.name(),
                "configured reasoning strategy"
            );
            Some(strategy)
        }
        Err(err) => {
            tracing::warn!(
                target: "synthia.agent",
                agent = %name,
                configured,
                error = %err,
                "unknown strategy in config; falling back to react"
            );
            None
        }
    }
}

// ---------------------------------------------------------------------------
// MCP server registration (R21/R30)
// ---------------------------------------------------------------------------

/// R21/R30: connect every enabled MCP server from the server
/// config through a [`synthia::mcp::McpSupervisor`] and publish
/// each server's tools into `registry` under namespaced names
/// (`mcp__<server>__<tool>`), so two servers may advertise the
/// same raw tool name.
///
/// One `tick` performs the boot connection sweep; the returned
/// health list covers every configured server, healthy or not.
/// Returns the live clients of the healthy servers — the caller
/// MUST keep them alive for the process lifetime, because
/// dropping a `StdioTransport` kills its child process
/// (`kill_on_drop`). A server that fails to spawn / handshake /
/// list is logged at `warn` and skipped, so one broken
/// integration cannot block boot.
pub(crate) async fn register_configured_mcp_servers(
    registry: &ToolRegistry,
    now: chrono::DateTime<chrono::Utc>,
    config: &crate::config::ServerConfig,
) -> (
    Vec<(String, Arc<synthia::mcp::McpClient>)>,
    Vec<synthia::mcp::ServerHealth>,
) {
    let supervisor = synthia::mcp::McpSupervisor::new();
    for server in config.mcp_servers.iter().filter(|s| s.enabled) {
        let factory = Arc::new(synthia::mcp::StdioTransportFactory::new(
            server.to_stdio_config(),
        ));
        supervisor.supervise(server.name.clone(), factory).await;
    }
    let tick = supervisor.tick(registry, now).await;
    log_tick_health(&tick.health);
    (supervisor.healthy_clients().await, tick.health)
}

/// Emit the boot-sweep log lines for every configured MCP server.
///
/// Walks the health slice in order. A healthy server is logged at
/// `info` with the count of registered tools; an unhealthy one is
/// logged at `warn` with its last state and consecutive-failure
/// count so an operator can decide whether to retry, restart the
/// child process, or drop the entry. The two log shapes keep the
/// orchestrator's loop flat.
fn log_tick_health(health: &[synthia::mcp::ServerHealth]) {
    for entry in health {
        if entry.healthy() {
            tracing::info!(
                target: "synthia.mcp",
                server = %entry.name,
                tools = entry.registered_tools,
                "registered MCP server tools"
            );
        } else {
            tracing::warn!(
                target: "synthia.mcp",
                server = %entry.name,
                state = ?entry.state,
                failures = entry.consecutive_failures,
                "MCP server unavailable; its tools are not registered"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// `[tools]` boot application (R34)
// ---------------------------------------------------------------------------

/// Apply the `[tools]` section to `registry` at boot.
///
/// The contract mirrors the MCP path: an optional integration that is
/// misconfigured must never block startup. A name the registry does
/// not have is dropped with a warning, as are an unknown
/// `active_groups` entry and a tool claimed by two groups — an MCP
/// server that failed to register must not take the server down. The
/// writes follow the documented order `groups` → `deferred` →
/// `hidden`, so a tool named by several keys ends with the later
/// treatment (`hidden` wins).
pub(crate) fn apply_tools_config(
    registry: &ToolRegistry,
    config: &crate::config::ToolsConfig,
) -> super::AppliedToolSurface {
    let mut skipped = Vec::new();
    let mut applied = super::AppliedToolSurface {
        max_visible: config.max_visible,
        groups: resolve_group_declarations(
            registry,
            &config.groups,
            &mut skipped,
        ),
        ..super::AppliedToolSurface::default()
    };

    activate_groups(&config.active_groups, &mut applied, &mut skipped);
    applied.skipped = skipped;
    if let Some(policy) = applied.policy()
        && let Err(error) = policy.apply(registry)
    {
        // Unreachable after the filtering above; kept visible so a
        // future divergence degrades loudly instead of silently
        // dropping the group state.
        tracing::warn!(
            target: "synthia.tools",
            %error,
            "tool surface: group policy was not applied"
        );
    }

    // Deferred, then hidden — a later key wins.
    applied.deferred = apply_surface_list(
        &config.deferred,
        &mut applied.skipped,
        "deferred",
        |name| {
            registry.set_exposure(name, synthia::tool::ToolExposure::Deferred)
        },
    );
    applied.hidden = apply_surface_list(
        &config.hidden,
        &mut applied.skipped,
        "hidden",
        |name| registry.set_hidden(name, true),
    );

    tracing::info!(
        target: "synthia.tools",
        deferred = applied.deferred.len(),
        hidden = applied.hidden.len(),
        groups = applied.groups.len(),
        active_groups = ?applied.active_groups,
        max_visible = ?applied.max_visible,
        skipped = ?applied.skipped,
        "tool surface applied from [tools] config"
    );
    applied
}

/// Activate the declared `active_groups` against `applied.groups`.
///
/// Walks `active_groups` in order and pushes the recognised names
/// onto `applied.active_groups`; an entry that was never declared
/// (or was filtered out by [`resolve_group_declarations`]) is
/// logged at `warn` and appended to `skipped` so the caller can
/// surface the boot-time drop. The verdict is later applied
/// through [`ToolSurfacePolicy`] when [`AppliedToolSurface::policy`]
/// is installed.
fn activate_groups(
    active_groups: &[String],
    applied: &mut super::AppliedToolSurface,
    skipped: &mut Vec<String>,
) {
    for group in active_groups {
        if applied.groups.contains_key(group) {
            applied.active_groups.push(group.clone());
        } else {
            tracing::warn!(
                target: "synthia.tools",
                group = %group,
                "tool surface: active group was never declared; skipped"
            );
            skipped.push(group.clone());
        }
    }
}

/// Resolve the `groups` declarations against `registry`: an unknown
/// member and a member already claimed by an earlier (name-order)
/// group are skipped with a warning and recorded in `skipped`. Never
/// fails — the boot-must-survive contract.
fn resolve_group_declarations(
    registry: &ToolRegistry,
    declared: &std::collections::BTreeMap<String, Vec<String>>,
    skipped: &mut Vec<String>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut resolved = std::collections::BTreeMap::new();
    let mut owner: std::collections::BTreeMap<&str, &str> =
        std::collections::BTreeMap::new();
    for (group, members) in declared {
        let mut kept = Vec::with_capacity(members.len());
        for member in members {
            if !registry.contains(member) {
                tracing::warn!(
                    target: "synthia.tools",
                    group = %group,
                    tool = %member,
                    "tool surface: group member is not registered; skipped"
                );
                skipped.push(member.clone());
                continue;
            }
            if let Some(existing) = owner.get(member.as_str()) {
                tracing::warn!(
                    target: "synthia.tools",
                    group = %group,
                    tool = %member,
                    existing = %existing,
                    "tool surface: tool already claimed by another group; \
                     claim skipped"
                );
                skipped.push(member.clone());
                continue;
            }
            owner.insert(member.as_str(), group.as_str());
            kept.push(member.clone());
        }
        resolved.insert(group.clone(), kept);
    }
    resolved
}

/// Apply `settle` to every name, returning the ones the registry
/// took. An unknown name is warned about and recorded in `skipped` —
/// the boot-must-survive contract shared by the `deferred` and
/// `hidden` lists.
fn apply_surface_list(
    names: &[String],
    skipped: &mut Vec<String>,
    list: &str,
    settle: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut applied = Vec::with_capacity(names.len());
    for name in names {
        if settle(name) {
            applied.push(name.clone());
        } else {
            tracing::warn!(
                target: "synthia.tools",
                tool = %name,
                list = %list,
                "tool surface: tool is not registered; skipped"
            );
            skipped.push(name.clone());
        }
    }
    applied
}

// ---------------------------------------------------------------------------
// Prompt-context builder
// ---------------------------------------------------------------------------

/// Build the prompt-context manifest the assembler consumes.
///
/// Skills are discovered via [`synthia::skill::discover_skills`],
/// which walks project + user roots and returns fully-resolved
/// [`synthia::skill::Skill`] values (mirrors opencode /
/// Anthropic Agent Skills convention). The same helper the
/// `skill` tool uses, so the prompt manifest and the runtime
/// always agree on what's available.
///
/// Peer agents are snapshotted from the live `AgentRegistry`
/// inline; the caller-supplied `self_name` is excluded so the
/// running agent never sees itself as a handoff target.
pub(crate) async fn build_prompt_context(
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
    for skill in synthia::skill::discover_skills(workspace_root) {
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
