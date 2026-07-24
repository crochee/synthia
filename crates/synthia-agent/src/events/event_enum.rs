//! The serde-tagged [`AgentEvent`] enum collapsed to five top-level
//! variants.
//!
//! - [`AgentEvent::Model`] — Provider [`synthia_provider::ContentPart`]
//!   pass-through for raw streaming chunks (text, reasoning, tool
//!   use, tool result, image, audio, resource).
//! - [`AgentEvent::ModelDone`] — Final aggregated
//!   [`synthia_provider::SamplingResult`].
//! - [`AgentEvent::System`] — Lifecycle and diagnostic state changes
//!   (see [`SystemEvent`]).
//! - [`AgentEvent::Agent`] — Recursive subagent trace wrapped with
//!   [`AgentMeta`].
//! - [`AgentEvent::Hook`] — External injection and custom events (see
//!   [`HookEvent`]).

use serde::{Deserialize, Serialize};
use synthia_provider::{ContentPart, SamplingResult};

use super::{
    agent_meta::AgentMeta,
    hook_event::HookEvent,
    system_event::{SystemEvent, WarningKind},
};

/// Events emitted by the agent during a session lifecycle. Serialized
/// with serde internally tagged for dispatch.
///
/// See module-level docs and the spec table in
/// `openspec/changes/simplify-agent-event-stream/specs/agent-event-bus/spec.md`
/// for the canonical wire format.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AgentEvent {
    /// A streaming chunk from the model.
    ///
    /// This is the pass-through carrier for `ContentPart` — text deltas,
    /// reasoning deltas, tool use, tool result, image, audio, resource —
    /// encoded verbatim from the Provider.
    Model(ContentPart),
    /// Final aggregated result of one model sampling pass (text, tool
    /// calls, reasoning, usage).
    ModelDone(SamplingResult),
    /// Lifecycle, diagnostic, and terminal state changes that are not
    /// user-visible streaming content.
    System(SystemEvent),
    /// A child (subagent) trace carrying the inner [`AgentEvent`] plus
    /// the [`AgentMeta`] that ties it back to its parent session.
    Agent(AgentMeta, Box<AgentEvent>),
    /// External injection (user steering, guardian confirmations) and
    /// extension custom events.
    Hook(HookEvent),
}

impl AgentEvent {
    /// Returns `true` if this event is durable (state-changing).
    ///
    /// Durable events must be replayed to reconstruct `LoopContext` or
    /// `TurnTask` state. Ephemeral events (`is_durable() == false`) are
    /// observable side-effects (streaming deltas, progress, warnings)
    /// that can be skipped during replay without affecting projected
    /// state.
    ///
    /// Per the `event-durability-classification` spec:
    /// - Durable: `Model(Text | ToolUse | ToolResult | Resource)`
    /// - Ephemeral: everything else, including `Model(Reasoning | Image | Audio)`,
    ///   `ModelDone`, every `System` variant, every `Hook` variant,
    ///   and the `Agent(meta, inner)` recursive wrapper (its inner
    ///   event's durability is decided by recursively unwrapping).
    pub fn is_durable(&self) -> bool {
        match self {
            Self::Model(ContentPart::Text(_))
            | Self::Model(ContentPart::ToolUse(_))
            | Self::Model(ContentPart::ToolResult(_))
            | Self::Model(ContentPart::Resource(_)) => true,
            Self::Model(_)
            | Self::ModelDone(_)
            | Self::System(_)
            | Self::Hook(_) => false,
            Self::Agent(_, inner) => inner.is_durable(),
        }
    }

    /// Convenience constructor for an ephemeral text delta.
    pub fn text_delta(text: impl Into<String>) -> Self {
        Self::Model(ContentPart::Text(synthia_provider::TextContent {
            text: text.into(),
            cache_control: None,
        }))
    }

    /// Convenience constructor for an ephemeral reasoning delta.
    pub fn reasoning_delta(
        text: impl Into<String>,
        signature: Option<String>,
    ) -> Self {
        Self::Model(ContentPart::Reasoning(
            synthia_provider::ReasoningContent {
                text: text.into(),
                signature,
            },
        ))
    }

    /// Convenience constructor for a system-level progress event.
    pub fn progress(
        message: impl Into<String>,
        step: usize,
        total: usize,
    ) -> Self {
        Self::System(SystemEvent::Progress {
            message: message.into(),
            step,
            total,
        })
    }

    /// Convenience constructor for a system-level warning.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::System(SystemEvent::Warning {
            kind: WarningKind::Hook,
            message: message.into(),
            iteration: None,
        })
    }

    /// Convenience constructor for a system-level warning of an
    /// arbitrary [`WarningKind`].
    pub fn warning_kind(kind: WarningKind, message: impl Into<String>) -> Self {
        Self::System(SystemEvent::Warning {
            kind,
            message: message.into(),
            iteration: None,
        })
    }

    /// Convenience constructor for a [`SystemEvent::Recovery`].
    pub fn recovery(
        level_number: u32,
        tool_name: Option<String>,
        message: impl Into<String>,
        iteration: Option<usize>,
    ) -> Self {
        Self::System(SystemEvent::Recovery {
            level_number,
            tool_name,
            message: message.into(),
            iteration,
        })
    }

    /// Convenience constructor for a [`SystemEvent::Usage`].
    pub fn usage(
        input_tokens: usize,
        output_tokens: usize,
        cache_read_tokens: Option<usize>,
        cache_creation_tokens: Option<usize>,
    ) -> Self {
        Self::System(SystemEvent::Usage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        })
    }
}
