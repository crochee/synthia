//! `AgentDescriptor` / `AgentFilter` / `AgentEntry` for the
//! [`synthia_agent::agent`] module.
//!
//! `AgentDescriptor` mirrors the de-facto industry shape used by
//! the Anthropic Agents SDK, the OpenAI Swarm/Agents SDK, and the
//! MCP-aligned reference designs.
//!
//! ## Identity (industry-aligned)
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

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;

use super::Agent;

/// Industry-aligned metadata for one registered agent.
///
/// Field semantics follow the Anthropic / OpenAI / MCP
/// conventions used across major agent frameworks (see module-level
/// docs). Fields are additive — older agents that only populated
/// the legacy subset (`name`, `description`, `kind`, `version`,
/// `capabilities`) keep working; new agents are encouraged to
/// fill in `instructions`, `tools`, `handoff_hint`, and
/// `persona`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentDescriptor {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub version: String,
    /// System prompt injected as the first message of every
    /// conversation. May be empty if the caller assembles the
    /// prompt at a higher layer.
    #[serde(default)]
    pub instructions: String,
    /// Capability tags (e.g. `"tools"`, `"streaming"`,
    /// `"cancellation"`). Free-form, used for filtering and
    /// routing.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Concrete tool names the agent can call directly. Mirrors
    /// OpenAI's `tools` field on the agent definition.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Preferred model identifier (e.g. `"claude-4.6"`,
    /// `"gpt-5"`). Provider-agnostic hint; downstream code is free
    /// to override.
    #[serde(default)]
    pub model_hint: Option<String>,
    /// Names of agents this specialist can hand off to. Mirrors
    /// OpenAI's `handoffs` field.
    #[serde(default)]
    pub handoffs: Vec<String>,
    /// Short label describing *when* an orchestrator should route
    /// to this agent. Mirrors OpenAI's `handoffDescription`.
    #[serde(default)]
    pub handoff_hint: Option<String>,
    /// Optional JSON-schema reference for structured outputs
    /// (`outputType`).
    #[serde(default)]
    pub output_schema: Option<String>,
    /// Owning team / service. Used for multi-tenant routing.
    #[serde(default)]
    pub owner: Option<String>,
    /// Functional domain (e.g. `"coding"`, `"research"`). Used
    /// for routing and observability.
    #[serde(default)]
    pub domain: Option<String>,
    /// Short role-framing sentence surfaced verbatim to the LLM
    /// (e.g. `"You are a strict security reviewer"`). Distinct
    /// from the long-form `instructions` block.
    #[serde(default)]
    pub persona: Option<String>,
    /// User-facing name rendered in the `<identity>` block and
    /// on the agent card. Mirrors the Anthropic /
    /// OpenCode distinction between the programmatic agent
    /// id (`name`, used for routing and registry keys) and the
    /// human-readable label the model and external clients
    /// see. `None` (and the empty string) falls back to
    /// [`Self::name`] — legacy descriptors that pre-date this
    /// field keep rendering exactly the same way they did
    /// before, since `#[serde(default)]` decodes missing
    /// values as `None`.
    #[serde(default)]
    pub display_name: Option<String>,
}

impl RegistryItem for AgentDescriptor {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}

impl AgentDescriptor {
    /// Human-readable name surfaced to the model (in the
    /// `<identity>` block) and to clients (on the
    /// `AgentCard`). Returns the trimmed `display_name`
    /// when set, otherwise falls back to the programmatic
    /// [`Self::name`]. The trim keeps " " / "\t" from
    /// rendering as a visible-but-blank identity label.
    pub fn display_name(&self) -> &str {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.name)
    }
}

/// Filter used by [`crate::agent::AgentRegistry::list_paginate`].
/// All fields are conjunctive (`AND`); `None` fields are ignored.
#[derive(Clone, Debug, Default)]
pub struct AgentFilter {
    pub kind: Option<String>,
    pub capability: Option<String>,
    pub tool: Option<String>,
    pub min_version: Option<String>,
    pub handoff: Option<String>,
    pub owner: Option<String>,
    pub domain: Option<String>,
}

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
    pub fn new(agent: Arc<dyn Agent>) -> Self {
        let descriptor = agent.descriptor().clone();
        Self { agent, descriptor }
    }

    pub fn descriptor(&self) -> &AgentDescriptor {
        &self.descriptor
    }

    /// Test-only mutable accessor for the cached descriptor.
    #[doc(hidden)]
    pub fn descriptor_mut(&mut self) -> &mut AgentDescriptor {
        &mut self.descriptor
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_descriptor() -> AgentDescriptor {
        AgentDescriptor {
            name: "agent".into(),
            description: "ReAct loop".into(),
            kind: "react".into(),
            version: "1.0.0".into(),
            instructions: "You are a coding assistant.".into(),
            capabilities: vec!["tools".into(), "streaming".into()],
            tools: vec!["read_file".into(), "shell".into()],
            model_hint: Some("claude-4.6".into()),
            handoffs: vec!["planner".into()],
            handoff_hint: Some("Use for code-editing tasks".into()),
            output_schema: None,
            owner: Some("synthia".into()),
            domain: Some("coding".into()),
            persona: Some("You are a pragmatic senior engineer.".into()),
            display_name: None,
        }
    }

    #[test]
    fn descriptor_name_description_roundtrip_via_registry_item() {
        let d = sample_descriptor();
        assert_eq!(d.name(), "agent");
        assert_eq!(d.description(), "ReAct loop");
    }

    #[test]
    fn descriptor_serde_roundtrip_preserves_instructions() {
        let d = sample_descriptor();
        let json = serde_json::to_string(&d).unwrap();
        let back: AgentDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.instructions, d.instructions);
        assert_eq!(back.tools, d.tools);
        assert_eq!(back.handoff_hint.as_deref(), d.handoff_hint.as_deref());
        assert_eq!(back.domain.as_deref(), d.domain.as_deref());
    }

    #[test]
    fn descriptor_serde_accepts_legacy_subset() {
        let json = r#"{
            "name": "x",
            "description": "y",
            "kind": "react",
            "version": "0.1.0",
            "capabilities": []
        }"#;
        let d: AgentDescriptor = serde_json::from_str(json).unwrap();
        assert_eq!(d.name, "x");
        assert!(d.instructions.is_empty());
        assert!(d.tools.is_empty());
        assert!(d.model_hint.is_none());
        assert!(d.persona.is_none());
        assert!(d.display_name.is_none());
    }

    /// `display_name` is the human-readable label; the
    /// helper returns it when set, otherwise falls back
    /// to the programmatic `name`. Empty / whitespace-only
    /// strings are treated as "not set" so a stray
    /// `display_name: ""` doesn't render a blank identity.
    #[test]
    fn display_name_falls_back_to_name_when_unset() {
        let mut d = sample_descriptor();
        d.display_name = None;
        assert_eq!(d.display_name(), "agent");

        d.display_name = Some("  ".into());
        assert_eq!(
            d.display_name(),
            "agent",
            "whitespace-only display_name must fall back to name"
        );

        d.display_name = Some(String::new());
        assert_eq!(
            d.display_name(),
            "agent",
            "empty display_name must fall back to name"
        );
    }

    #[test]
    fn display_name_overrides_when_set() {
        let mut d = sample_descriptor();
        d.display_name = Some("Synthia".into());
        assert_eq!(d.display_name(), "Synthia");
    }

    #[test]
    fn descriptor_serde_roundtrip_preserves_display_name() {
        let mut d = sample_descriptor();
        d.display_name = Some("Synthia".into());
        let json = serde_json::to_string(&d).unwrap();
        let back: AgentDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.display_name.as_deref(), Some("Synthia"));
    }

    #[test]
    fn filter_default_matches_everything() {
        let _f = AgentFilter::default();
    }

    /// Forward-compatibility contract: payloads produced by
    /// a *future* build of `synthia-agent` that has added
    /// NEW fields the current build doesn't know about MUST
    /// still deserialize successfully, with the unknown
    /// fields silently dropped.
    #[test]
    fn descriptor_serde_tolerates_unknown_fields() {
        let json = r#"{
            "name": "future-agent",
            "description": "x",
            "kind": "react",
            "version": "1.0.0",
            "capabilities": [],
            "tools": [],
            "future_field_only_new_build_knows": "ignored",
            "future_nested": {
                "deep": "ignored"
            }
        }"#;
        let d: AgentDescriptor = serde_json::from_str(json).unwrap();
        assert_eq!(d.name, "future-agent");
        assert!(d.tools.is_empty());
    }

    /// JSON object keys are unordered by spec. If a future
    /// refactor accidentally uses a non-Roundtrip-safe serde
    /// representation (e.g. by adding a tuple-struct or a
    /// Map-backed struct), this test catches it.
    #[test]
    fn descriptor_serde_is_key_order_independent() {
        let a = r#"{
            "name": "x", "description": "y", "kind": "k",
            "version": "1", "capabilities": [], "tools": []
        }"#;
        let b = r#"{
            "tools": [], "capabilities": [], "version": "1",
            "kind": "k", "description": "y", "name": "x"
        }"#;
        let pa: AgentDescriptor = serde_json::from_str(a).unwrap();
        let pb: AgentDescriptor = serde_json::from_str(b).unwrap();
        assert_eq!(pa.name, pb.name);
        assert_eq!(pa.kind, pb.kind);
        assert_eq!(pa.version, pb.version);
        assert_eq!(pa.tools, pb.tools);
    }
}
