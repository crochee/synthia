//! `AgentEntry` for the `synthia_harness::agent` module.
//!
//! [`AgentDescriptor`] / [`AgentFilter`] moved to
//! [`synthia_core::agent`] (re-exported below for back-compat) —
//! they are pure data that cross-crate consumers (registry
//! filters, delegation peers, server routes, eval harnesses) name
//! without a transitive `synthia-harness` dependency. `AgentEntry`
//! stays here because it owns an `Arc<dyn Agent>` and therefore a
//! hard `synthia-harness` dependency.
//!
//! ## Identity (industry-aligned)
//!
//! `AgentDescriptor` mirrors the de-facto industry shape used by
//! the Anthropic Agents SDK, the OpenAI Swarm/Agents SDK, and the
//! MCP-aligned reference designs.
//!
//! - `name` / `description` — identity (Anthropic `name`, OpenAI
//!   `name` + `handoffDescription`).
//! - `kind` / `version` — paradigm + schema revision.
//! - `instructions` — the system prompt (Anthropic `instructions`,
//!   OpenAI `instructions`).
//! - `model_hint` — preferred model identifier (Anthropic `model`,
//!   OpenAI `model`).
//! - `tools` — tool names exposed directly (OpenAI `tools`).
//! - `capabilities` — coarse capability tags ("streaming",
//!   "cancellation", …); finer-grained than `tools`.
//! - `handoffs` — agent names this specialist can route to
//!   (OpenAI `handoffs`).
//! - `handoff_hint` — short label describing *when* an
//!   orchestrator should route to this agent (OpenAI
//!   `handoffDescription`).
//! - `output_schema` — optional JSON-schema reference for
//!   structured outputs (OpenAI `outputType`).
//! - `owner` / `domain` — ownership + functional domain, useful
//!   for multi-tenant routing.
//! - `persona` — short role-framing sentence surfaced verbatim to
//!   the LLM (e.g. `"You are a strict security reviewer"`).
//!   Distinct from the long-form `instructions`.
//!
//! ## Why no `panel` / `role` / `debate_protocol`?
//!
//! The previous descriptor carried an **adversarial-panel**
//! model (`AdversarialRole` × `DebateProtocol`) where multiple
//! agents of the same panel coordinated via a coordinator.
//! That model conflated two separate concerns:
//!
//! - **Multi-agent orchestration** — a runtime concern that
//!   chooses which agents to invoke.
//! - **Agent identity** — the descriptor is the static
//!   metadata a developer writes into `agents.toml`.
//!
//! After this refactor the descriptor is purely **identity +
//! capability**. Panel membership and orchestration strategy
//! are runtime policy held by the orchestrator (server-side
//! callers can compose `delegate` / `resume_with_agent`
//! primitives as needed). Agents no longer know they are part
//! of a panel.

// Re-exports — kept here so every consumer that wrote
// `use synthia_harness::agent::AgentDescriptor` (or
// the crate-root `AgentDescriptor`) keeps compiling unchanged
// after the move to `synthia_core::agent`.
use std::sync::Arc;

pub use synthia_core::agent::{AgentDescriptor, AgentFilter};
use synthia_core::registry::RegistryItem;

use super::Agent;

/// Pairing of an `Arc<dyn Agent>` and a cached [`AgentDescriptor`].
///
/// Constructed by the registry when an agent is registered; the
/// descriptor is cloned out of the runtime agent so a route
/// handler / orchestrator can read the metadata without a vtable
/// hop. The cache is write-once (registration time); the
/// `descriptor_mut` accessor is crate-internal and test-only.
#[derive(Clone)]
pub struct AgentEntry {
    agent: Arc<dyn Agent>,
    descriptor: AgentDescriptor,
}

impl std::fmt::Debug for AgentEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentEntry")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl AgentEntry {
    /// Build one entry by cloning the runtime agent's descriptor.
    #[must_use]
    pub fn new(agent: Arc<dyn Agent>) -> Self {
        let descriptor = agent.descriptor().clone();
        Self { agent, descriptor }
    }

    #[must_use]
    pub fn descriptor(&self) -> &AgentDescriptor {
        &self.descriptor
    }

    /// Test-only mutable accessor for the cached descriptor: the
    /// registry's own tests seed a version / tool list / owner by hand.
    ///
    /// `#[cfg(test)] pub(crate)`, not `pub` — no consumer outside this
    /// crate has a use for a write handle on a cache that is otherwise
    /// write-once at registration, and production builds do not need it
    /// to exist. (The same-named
    /// [`ReActAgent::descriptor_mut`](crate::agent::ReActAgent::descriptor_mut)
    /// *is* public: it replaces a running agent's descriptor wholesale,
    /// and `synthia-tool-task`'s tests call it.)
    #[cfg(test)]
    pub(crate) fn descriptor_mut(&mut self) -> &mut AgentDescriptor {
        &mut self.descriptor
    }

    #[must_use]
    pub fn agent(&self) -> Arc<dyn Agent> {
        Arc::clone(&self.agent)
    }
}

impl RegistryItem for AgentEntry {
    fn name(&self) -> &str {
        self.descriptor.name()
    }

    fn description(&self) -> &str {
        self.descriptor.description()
    }
}
