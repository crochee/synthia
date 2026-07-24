//! Agent lifecycle events.
//!
//! # Module Layout
//!
//! - [`reasons`]: [`reasons::SessionEndReason`] (terminal reason) +
//!   [`reasons::ErrorSource`] + [`reasons::TurnEndReason`] +
//!   [`reasons::AgentStatus`] + [`reasons::ProgressEvent`] +
//!   [`reasons::ErrorEvent`].
//! - [`system_event`][]: the [`system_event::SystemEvent`] enum +
//!   [`system_event::WarningKind`] for lifecycle and diagnostic events.
//! - [`hook_event`][]: the [`hook_event::HookEvent`] enum for external
//!   injection and custom events.
//! - [`agent_meta`][]: the [`agent_meta::AgentMeta`] struct describing
//!   a subagent's parent / child relationship.
//! - [`event_enum`][]: the top-level [`event_enum::AgentEvent`] enum
//!   collapsed to five variants (`Model` / `ModelDone` / `System` /
//!   `Agent` / `Hook`).
//! - [`emitter`][]: the unbounded-MPSC [`emitter::AgentEventEmitter`]
//!   + its `Clone` impl.
//! - [`tests`]: unit tests covering the serde round trip, helper
//!   ctors, emitter pair / clone / drop, and the wire format of
//!   `Recovery`.

mod agent_meta;
mod emitter;
mod event_enum;
mod hook_event;
mod persisted;
mod reasons;
mod system_event;

#[cfg(test)]
mod tests;

pub use agent_meta::AgentMeta;
pub use emitter::AgentEventEmitter;
pub use event_enum::AgentEvent;
pub use hook_event::HookEvent;
pub use persisted::{
    SAMPLE_COMPLETED,
    SESSION_ENDED,
    TOOL_CALL_ISSUED,
    TOOL_RESULT_RECEIVED,
    TURN_COMPLETED,
    TURN_FAILED,
    TURN_STARTED,
    append_agent_event,
    is_durable_event_type,
    read_all_events,
    session_path,
};
pub use reasons::{
    AgentStatus,
    ErrorEvent,
    ErrorSource,
    ProgressEvent,
    SessionEndReason,
    TurnEndReason,
};
/// Re-export of the canonical [`synthia_provider::types::TokenUsage`] (4
/// fields including `cached_prompt_tokens`). All token-usage values emitted
/// through `AgentEvent` use this single type.
pub use synthia_provider::types::TokenUsage;
pub use system_event::{SystemEvent, WarningKind};

/// A stream of [`AgentEvent`]s produced by the agent run loop.
pub type AgentOutput = Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>>;

use std::pin::Pin;
