//! The serde-tagged [`AgentEvent`] enum + 3 helper
//! constructors ([`AgentEvent::thinking`] /
//! [`AgentEvent::progress`] / [`AgentEvent::warning`]).

use serde::{Deserialize, Serialize};

use super::{
    TokenUsage,
    reasons::{AgentStatus, SessionEndReason},
};

/// Events emitted by the agent during a session
/// lifecycle. Serialized with serde internally tagged
/// for dispatch.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AgentEvent {
    SessionStarted {
        session_id: String,
    },
    SessionEnded {
        reason: SessionEndReason,
    },
    LlmRequestStarted {
        iteration: usize,
    },
    LlmStreamDelta {
        content: String,
    },
    LlmReasoningDelta {
        delta: String,
    },
    LlmResponseComplete {
        content: String,
        usage: TokenUsage,
    },
    LlmError {
        error: String,
    },
    ToolCallStarted {
        tool_name: String,
        input: serde_json::Value,
    },
    ToolCallCompleted {
        tool_name: String,
        output: String,
        is_error: bool,
    },
    ToolCallSkipped {
        tool_name: String,
        reason: String,
    },
    ToolCallError {
        tool_name: String,
        error: String,
    },
    IterationStarted {
        iteration: usize,
    },
    IterationCompleted {
        iteration: usize,
    },
    Thinking {
        text: String,
        iteration: usize,
    },
    ContextCompacted {
        old_tokens: usize,
        new_tokens: usize,
    },
    Checkpoint {
        session_id: String,
        step: usize,
    },
    StateChange {
        from: String,
        to: String,
    },
    Warning {
        message: String,
    },
    Progress {
        message: String,
        step: usize,
        total: usize,
    },
    SessionInterrupted {
        reason: String,
    },
    Finish {
        output: String,
    },
    GuardianWarning {
        reason: String,
        iteration: usize,
    },
    LoopWarning {
        reason: String,
        iteration: usize,
    },
    /// Recovery action applied during the agent loop. Emitted for every
    /// L1 truncation, L3 fallback, L4 compact, and L5 reset so external
    /// observers (telemetry, UI, tests) can see *why* the session did
    /// not abort despite a tool/LLM error.
    ///
    /// `level_number`: 1 = Truncate, 2 = Retry, 3 = Fallback, 4 = Compact,
    ///                 5 = Reset. `u32` is used instead of
    /// `crate::error_recovery::RecoveryLevel` to keep the public event
    /// wire format stable and independent of the recovery module's
    /// internal enums (which the archive specs constrain).
    ///
    /// `tool_name`: `Some(name)` for tool-specific recovery; the LLM
    /// sampling path uses the synthetic `Some("llm_sample")` so the
    /// field is never `None` (preserves the spec invariant
    /// "tool_name is Some('llm_sample') for LLM-only recovery").
    RecoveryApplied {
        level_number: u32,
        tool_name: Option<String>,
        message: String,
        iteration: usize,
    },
    TokenBudgetNotice {
        status: String,
        current_tokens: usize,
        threshold_tokens: usize,
    },
    TokenBudgetWarning {
        status: String,
        current_tokens: usize,
        threshold_tokens: usize,
    },
    SteeringReceived {
        message: String,
        session_id: String,
        /// Steering priority, if available. Preserved from the queued
        /// input so consumers can observe or re-sort by urgency.
        #[serde(default)]
        priority: Option<i32>,
    },
    HookError {
        hook_name: String,
        error: String,
        hook_type: String,
    },
    GuardianConfirmationRequest {
        tool_name: String,
        reason: String,
    },
    EditConflict {
        tool_name: String,
        call_id: String,
        path: String,
        original_content_hash: u64,
        current_content_hash: u64,
    },
    SelfReflection {
        iteration: usize,
        summary: String,
        issues: Vec<String>,
        suggestions: Vec<String>,
    },

    // Subagent lifecycle events (Phase 5)
    SubagentSpawnBegin {
        session_id: String,
        agent_path: String,
    },
    SubagentSpawnEnd {
        session_id: String,
        agent_path: String,
        success: bool,
        error: Option<String>,
    },
    SubagentMessage {
        session_id: String,
        agent_path: String,
        message: String,
    },
    /// Emitted for inline (foreground) subagent completion. Carries the
    /// full result and `agent_path` for direct callers.
    ///
    /// Distinct from [`AgentEvent::SubagentCompleted`], which is an
    /// ephemeral best-effort notification with a truncated summary sent
    /// to the parent's event stream when a (typically background)
    /// subagent finishes.
    SubagentComplete {
        session_id: String,
        agent_path: String,
        result: String,
    },
    /// Emitted when a background subagent completes (success or error).
    /// Wrapped in [`AgentEvent::SubagentEvent`] and forwarded to the
    /// parent's event stream via
    /// [`crate::subagent::ChildSessionHandle::parent_event_sender`].
    ///
    /// `result_summary` is the first 500 characters of the subagent's
    /// final output or error message, truncated at a valid UTF-8
    /// boundary with a trailing `"… [truncated]"` indicator when
    /// truncation occurred (per the `subagent-background-mode` spec).
    ///
    /// This is an ephemeral notification: it is NOT durable (the parent
    /// does not need to replay it to reconstruct state) and is distinct
    /// from [`AgentEvent::SubagentComplete`], which carries the full
    /// result for inline (foreground) callers.
    SubagentCompleted {
        session_id: String,
        result_summary: String,
    },
    /// A raw event emitted by a child (subagent) session, wrapped so
    /// that observers of the parent session see the whole session tree
    /// without opening multiple connections.
    #[serde(rename = "subagent_event")]
    SubagentEvent {
        child_session_id: String,
        event: Box<AgentEvent>,
    },
    /// Agent status change event.
    Status(AgentStatus),
}

impl AgentEvent {
    /// Returns `true` if this event is durable (state-changing).
    ///
    /// Durable events must be replayed to reconstruct `LoopContext` or
    /// `TurnTask` state. Ephemeral events (`is_durable() == false`) are
    /// observable side-effects (streaming deltas, progress, warnings) that
    /// can be skipped during replay without affecting projected state.
    pub fn is_durable(&self) -> bool {
        matches!(
            self,
            Self::SessionStarted { .. }
                | Self::SessionEnded { .. }
                | Self::LlmRequestStarted { .. }
                | Self::LlmResponseComplete { .. }
                | Self::ToolCallStarted { .. }
                | Self::ToolCallCompleted { .. }
                | Self::ToolCallSkipped { .. }
                | Self::ToolCallError { .. }
                | Self::IterationStarted { .. }
                | Self::ContextCompacted { .. }
                | Self::Checkpoint { .. }
                | Self::StateChange { .. }
                | Self::RecoveryApplied { .. }
                | Self::Status(..)
                | Self::SteeringReceived { .. }
                | Self::GuardianConfirmationRequest { .. }
                | Self::SubagentSpawnBegin { .. }
                | Self::SubagentSpawnEnd { .. }
                | Self::SubagentComplete { .. }
                | Self::Finish { .. }
        )
    }

    pub fn thinking(text: impl Into<String>, iteration: usize) -> Self {
        Self::Thinking {
            text: text.into(),
            iteration,
        }
    }

    pub fn progress(
        message: impl Into<String>,
        step: usize,
        total: usize,
    ) -> Self {
        Self::Progress {
            message: message.into(),
            step,
            total,
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::Warning {
            message: message.into(),
        }
    }
}
