//! Agent lifecycle events.
//!
//! # Module Layout
//!
//! - [`reasons`]: 3 reason / status enums
//!   ([`reasons::SessionEndReason`] +
//!   [`reasons::ErrorSource`] +
//!   [`reasons::TurnEndReason`] +
//!   [`reasons::AgentStatus`] +
//!   [`reasons::ProgressEvent`] + [`reasons::ErrorEvent`]).
//! - [`event_enum`][]: the
//!   [`event_enum::AgentEvent`] serde-tagged enum
//!   (the ~30 lifecycle variants — `SessionStarted` /
//!   `LlmRequestStarted` / `ToolCallStarted` /
//!   `RecoveryApplied` / `SubagentSpawnBegin` / etc.).
//! - [`emitter`][]: the
//!   [`emitter::AgentEventEmitter`] unbounded-MPSC
//!   emitter + its `Clone` impl.
//! - [`tests`]: 10 unit tests covering the serde round
//!   trip, helper ctors, emitter pair / clone / drop, and
//!   the wire format of `RecoveryApplied`.

mod emitter;
mod event_enum;
mod persisted;
mod reasons;

#[cfg(test)]
mod tests;

pub use emitter::AgentEventEmitter;
pub use event_enum::AgentEvent;
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

/// A stream of [`AgentEvent`]s produced by the agent run loop.
pub type AgentOutput = Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>>;

use std::pin::Pin;
