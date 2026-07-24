//! Outcome types for the per-iteration tool execution phase.

use crate::events::AgentEvent;

// ---------- Outcome enum ----------

/// Outcome of the per-iteration tool execution phase.
///
/// The caller (the `stream!` block in [`super::run`])
/// pattern-matches on the variant, yields the contained
/// events, and either `continue`s to the next iteration
/// or `return`s from the stream.
pub enum ToolExecuteOutcome {
    /// Tool execution finished (with or without cascade
    /// recovery). The caller yields the contained events
    /// — `ToolCallStarted` per call, optionally a
    /// `RecoveryApplied` from the cascade, `ToolCallCompleted`
    /// per result, optionally `RecoveryApplied { level: 1 }`
    /// from L1 truncation — and continues to the next
    /// iteration.
    Continue { events: Vec<AgentEvent> },
    /// The recovery cascade exhausted
    /// ([`RecoveryAction::FailFast`] /
    /// [`RecoveryAction::Escalate`]). The caller yields
    /// the contained `SessionEnded` event and `return`s
    /// from the stream.
    Terminate { events: Vec<AgentEvent> },
}
