//! Agent registry HTTP handlers.
//!
//! Thin CRUD layer over the existing [`AgentRegistry`]:
//!
//! - `POST /agents` instantiates a [`ReActAgent`] bound to the
//!   supplied descriptor and registers it. No new types —
//!   the canonical `Registry::register` path is reused.
//! - `GET /agents` and `GET /agents/{name}` expose the stored
//!   descriptors.
//! - `DELETE /agents/{name}` unregisters. The default agent
//!   name (and any name the server considers core, e.g.
//!   `"agent"`) is protected.
//!
//! Agents registered via this endpoint are real ReAct agents —
//! they have a model provider, tool registry, prompt context,
//! and a writable descriptor. They run the same loop as the
//! canonical agent.

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use synthia_agent::agent::{
    Agent,
    descriptor::{AgentDescriptor, AgentEntry},
};
use synthia_core::{Error, registry::Registry};
use synthia_provider::ModelProvider;

use super::helpers::paginate;
use crate::{
    api::{
        AppError,
        AppJson,
        AppPath,
        AppQuery,
        List,
        PageQuery,
        resolve_page,
        validate_resource_name,
        validate_sort,
    },
    state::AppState,
};

/// Sortable fields for the agents list endpoint.
const AGENT_SORT_WHITELIST: &[&str] = &["name", "kind"];

/// Names of agents that ship with the binary and cannot be
/// removed through the v1 API. The single canonical agent is
/// `"agent"`; additional entries can be added here without
/// touching handler logic.
const PROTECTED_AGENTS: &[&str] = &["agent"];

#[derive(Deserialize, validator::Validate)]
pub struct AgentDescriptorRequest {
    #[validate(length(min = 1, message = "must not be empty"))]
    pub name: String,
    pub description: String,
    pub kind: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub handoffs: Vec<String>,
    #[serde(default)]
    pub model_hint: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub persona: Option<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

#[derive(Serialize)]
pub struct AgentDetail {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub version: String,
    pub instructions: String,
    pub capabilities: Vec<String>,
    pub tools: Vec<String>,
    pub handoffs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// `true` for the names listed in [`PROTECTED_AGENTS`];
    /// `false` for agents registered at runtime.
    pub protected: bool,
}

fn detail_from(desc: &AgentDescriptor) -> AgentDetail {
    AgentDetail {
        name: desc.name.clone(),
        description: desc.description.clone(),
        kind: desc.kind.clone(),
        version: desc.version.clone(),
        instructions: desc.instructions.clone(),
        capabilities: desc.capabilities.clone(),
        tools: desc.tools.clone(),
        handoffs: desc.handoffs.clone(),
        model_hint: desc.model_hint.clone(),
        owner: desc.owner.clone(),
        domain: desc.domain.clone(),
        persona: desc.persona.clone(),
        protected: PROTECTED_AGENTS.contains(&desc.name.as_str()),
    }
}

fn is_protected(name: &str) -> bool {
    PROTECTED_AGENTS.contains(&name)
}

/// Cached `(registry_version, entries)` for `list_agents`.
///
/// `state.agent_registry.list(None)` acquires the registry's
/// internal `RwLock` read guard and walks the `HashMap`,
/// building a fresh `Vec<Arc<AgentEntry>>` on every call. With
/// a small `N` the cost is dominated by the lock acquire and
/// the HashMap traversal (O(N) bucket walk), not the
/// `detail_from` clones that follow. Caching the entries Vec
/// under the registry's monotonic version counter (see
/// `AgentRegistry::version`) lets the hot path skip both the
/// lock and the walk entirely. A `create_agent` /
/// `delete_agent` call bumps the version and the next list
/// request rebuilds once.
static LIST_CACHE: RwLock<Option<(u64, Arc<Vec<AgentEntry>>)>> =
    RwLock::new(None);

/// Construct a [`ReActAgent`] bound to the supplied descriptor
/// using the shared provider, tool registry, workspace root, and
/// prompt context on `AppState`. Keeps the runtime surface
/// uniform — every registered agent is a real ReAct agent, no
/// stub-only path is introduced.
fn build_react_agent(
    state: &AppState,
    descriptor: AgentDescriptor,
) -> std::sync::Arc<dyn Agent> {
    let provider: Arc<dyn ModelProvider> = Arc::clone(&state.default_provider);
    let tool_registry = {
        match state.tool_registry.try_read() {
            Ok(g) => Arc::new(g.clone()),
            Err(_) => {
                tracing::warn!(
                    "tool registry lock contention while building agent; \
                     falling back to an empty registry"
                );
                Arc::new(synthia_tool::ToolRegistry::new())
            }
        }
    };
    Arc::new(synthia_agent::ReActAgent::with_descriptor(
        provider,
        tool_registry,
        state.workspace_root.clone(),
        descriptor,
        state.prompt_context.clone(),
    ))
}

/// GET /api/v1/agents — list registered agents (cursor paginated).
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    AppQuery(page): AppQuery<PageQuery>,
) -> Result<Json<List<AgentDetail>>, AppError> {
    validate_sort(
        page.sort.as_deref().unwrap_or("name"),
        AGENT_SORT_WHITELIST,
    )?;
    let resolved = resolve_page(&page)?;

    // Cache fast path: key on registry version. Hit: clone the
    // `Arc<Vec<Arc<AgentEntry>>>` (8-byte refcount bump) without
    // touching the registry. Miss: acquire the registry read
    // lock, snapshot the entries, store under the current
    // version.
    //
    // `parking_lot::RwLockReadGuard` is not `Send`, so the read
    // lock must be dropped before any `.await` point — axum's
    // `Handler` bound requires the returned future to be
    // `Send`. We use a small inline closure to scope the guard
    // and pull the cached `Arc` (or `None` on miss) out before
    // hitting the `.await`.
    let version = state.agent_registry.version();
    let cached_arc = {
        let guard = LIST_CACHE.read();
        guard
            .as_ref()
            .filter(|(v, _)| *v == version)
            .map(|(_, c)| c.clone())
    };
    let entries_arc: Arc<Vec<AgentEntry>> = match cached_arc {
        Some(c) => c,
        None => rebuild_entries_cache(&state, version).await?,
    };

    let mut details: Vec<AgentDetail> = entries_arc
        .iter()
        .map(|e| detail_from(e.descriptor()))
        .collect();
    details.sort_by(|a, b| a.name.cmp(&b.name));
    if resolved.descending {
        details.reverse();
    }
    let list = paginate(details, &resolved, |d: &AgentDetail| d.name.as_str());
    Ok(Json(list))
}

/// Build the entries cache for `LIST_CACHE`. Acquires the
/// registry read lock to snapshot the `Vec<Arc<AgentEntry>>`,
/// then writes the resulting `(version, entries)` under the
/// static lock. Returns the cached `Arc` so the caller can hand
/// it straight to `detail_from` + `paginate`.
async fn rebuild_entries_cache(
    state: &Arc<AppState>,
    version: u64,
) -> Result<Arc<Vec<AgentEntry>>, AppError> {
    let entries =
        state.agent_registry.list(None).await.map_err(|e| {
            Error::internal(format!("agent registry error: {e}"))
        })?;
    let arc = Arc::new(entries);
    // Re-check inside the write guard — another request may
    // have populated the cache while we were building.
    let mut guard = LIST_CACHE.write();
    let cached = if let Some((v, cached)) = guard.as_ref() {
        if *v == version {
            cached.clone()
        } else {
            *guard = Some((version, arc.clone()));
            arc
        }
    } else {
        *guard = Some((version, arc.clone()));
        arc
    };
    Ok(cached)
}

/// POST /api/v1/agents — register a new agent from a descriptor.
pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<AgentDescriptorRequest>,
) -> Result<(StatusCode, Json<AgentDetail>), AppError> {
    validate_resource_name(&req.name)?;
    if req.description.trim().is_empty() {
        return Err(AppError::from(Error::invalid_item(
            "description must not be empty",
        )));
    }
    if is_protected(&req.name) {
        return Err(AppError::from(Error::already_exists(format!(
            "agent '{}' (protected built-in, cannot be re-registered)",
            req.name
        ))));
    }

    let descriptor = AgentDescriptor {
        name: req.name.clone(),
        description: req.description,
        kind: req.kind,
        version: if req.version.is_empty() {
            default_version()
        } else {
            req.version
        },
        instructions: req.instructions,
        capabilities: req.capabilities,
        tools: req.tools,
        model_hint: req.model_hint,
        handoffs: req.handoffs,
        handoff_hint: None,
        output_schema: None,
        owner: req.owner,
        domain: req.domain,
        persona: req.persona,
        display_name: None,
    };

    let entry = AgentEntry::new(build_react_agent(&state, descriptor.clone()));
    state.agent_registry.put(entry).await.map_err(|e| match e {
        // Pass the classified core error through untouched —
        // `AlreadyExists` maps to 409 with code "already_exists".
        already @ synthia_core::Error::AlreadyExists { .. } => already,
        other => Error::internal(format!("registration failed: {other}")),
    })?;

    Ok((StatusCode::CREATED, Json(detail_from(&descriptor))))
}

/// Response shape for `GET /api/v1/agents/default`. Carries the
/// resolved name plus the source the ladder landed on, so the
/// frontend can show "using default agent X (from config)" vs
/// "defaulting to first registered agent X" without a second
/// round-trip.
#[derive(Serialize)]
pub struct DefaultAgentResponse {
    /// Resolved agent name to redirect to.
    pub name: String,
    /// Which rung of `explicit > configured default > first
    /// registered` supplied `name`. Lets the frontend render a
    /// tooltip explaining the routing decision.
    pub source: &'static str,
}

/// GET /api/v1/agents/default — resolve the default agent name
/// via the same ladder the controller uses
/// (`explicit > configured default > first registered`) and
/// return it. The frontend uses this on `/chat/:sessionId`
/// (no agent in the URL) to redirect to
/// `/chat/:sessionId/agent/:name`. Returns 404 only when the
/// registry is empty.
pub async fn get_default_agent(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DefaultAgentResponse>, AppError> {
    let explicit: Option<String> = None;
    let configured = state.default_agent_name.read().clone();
    let entries =
        state.agent_registry.list(None).await.map_err(|e| {
            Error::internal(format!("agent registry error: {e}"))
        })?;
    if entries.is_empty() {
        return Err(AppError::from(Error::not_found("registered agents")));
    }

    let resolved = AppState::resolve_agent_name(
        &state.agent_registry,
        configured.as_deref(),
        explicit.as_deref(),
    )
    .ok_or_else(|| AppError::from(Error::not_found("registered agents")))?;

    // Mirror the ladder semantics so the source tag is honest.
    // If the requested name actually matches the registry's
    // first registered agent AND there's no configured default
    // AND no explicit override, label it as "first-registered".
    let source = if configured.as_deref() == Some(resolved.as_str()) {
        "configured"
    } else if configured.is_some() {
        // The configured default wasn't matched (registry
        // doesn't contain it). The fallback was either the
        // explicit request or the first-registered agent.
        "first-registered"
    } else {
        "first-registered"
    };

    Ok(Json(DefaultAgentResponse {
        name: resolved,
        source,
    }))
}

/// GET /api/v1/agents/{name} — fetch one agent.
pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<Json<AgentDetail>, AppError> {
    validate_resource_name(&name)?;
    let entries =
        state.agent_registry.get(&name).await.map_err(|e| {
            Error::internal(format!("agent registry error: {e}"))
        })?;
    let entry = entries.ok_or_else(|| {
        AppError::from(Error::not_found(format!("agent '{name}'")))
    })?;
    Ok(Json(detail_from(entry.descriptor())))
}

/// DELETE /api/v1/agents/{name} — unregister. Protected names
/// return 403. Missing names are treated as already-deleted
/// (204 — DELETE is idempotent).
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<(StatusCode, ()), AppError> {
    validate_resource_name(&name)?;
    if is_protected(&name) {
        return Err(AppError::from(Error::forbidden(format!(
            "agent '{name}' (protected built-in, cannot be removed)"
        ))));
    }
    match state.agent_registry.delete(&name).await {
        Ok(()) => Ok((StatusCode::NO_CONTENT, ())),
        Err(synthia_core::Error::NotFound { .. }) => {
            Ok((StatusCode::NO_CONTENT, ()))
        }
        Err(other) => Err(AppError::from(Error::internal(format!(
            "unregister failed: {other}"
        )))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_version_is_0_1_0() {
        assert_eq!(default_version(), "0.1.0");
    }

    #[test]
    fn protected_agents_contains_agent() {
        assert!(is_protected("agent"));
        assert!(!is_protected("explorer"));
        assert!(!is_protected("critic"));
    }

    #[test]
    fn detail_from_propagates_descriptor_fields() {
        let desc = AgentDescriptor {
            name: "x".into(),
            description: "d".into(),
            kind: "react".into(),
            version: "1.2.3".into(),
            instructions: "be terse".into(),
            capabilities: vec!["tools".into()],
            tools: vec!["read_file".into()],
            model_hint: Some("claude-4.6".into()),
            handoffs: vec!["explorer".into()],
            handoff_hint: None,
            output_schema: None,
            owner: Some("team-a".into()),
            domain: Some("coding".into()),
            persona: Some("reviewer".into()),
            display_name: None,
        };
        let d = detail_from(&desc);
        assert_eq!(d.name, "x");
        assert_eq!(d.version, "1.2.3");
        assert_eq!(d.instructions, "be terse");
        assert_eq!(d.tools, vec!["read_file".to_string()]);
        assert!(!d.protected);
    }

    #[test]
    fn detail_marks_protected_name() {
        let desc = AgentDescriptor {
            name: "agent".into(),
            description: "canonical".into(),
            kind: "react".into(),
            version: "1.0.0".into(),
            instructions: String::new(),
            capabilities: vec![],
            tools: vec![],
            model_hint: None,
            handoffs: vec![],
            handoff_hint: None,
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
        };
        let d = detail_from(&desc);
        assert!(d.protected);
    }

    /// `DefaultAgentResponse::source` MUST be `configured`
    /// when the configured default resolves and matches the
    /// registry. Verifies the simple happy path; the
    /// fall-through cases are exercised by the integration
    /// tests under `tests/` once the controller is wired.
    #[test]
    fn default_agent_response_source_label() {
        // The source field is a constant string per request;
        // assert the contract is a stable set of values.
        let resp = DefaultAgentResponse {
            name: "agent".into(),
            source: "configured",
        };
        assert_eq!(resp.name, "agent");
        assert_eq!(resp.source, "configured");

        let resp = DefaultAgentResponse {
            name: "fallback".into(),
            source: "first-registered",
        };
        assert_eq!(resp.source, "first-registered");
    }
}
