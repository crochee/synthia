//! [`SystemEvent`] + [`WarningKind`] definitions and the
//! [`SessionEndReason`] re-export.
//
// [`SessionEndReason`]: super::reasons::SessionEndReason

use serde::{Deserialize, Serialize};

use super::reasons::SessionEndReason;

/// Lifecycle, diagnostic, and terminal state changes reported via
/// [`AgentEvent::System`](super::AgentEvent::System).
///
/// Spec table (durable = true):
///
/// | Variant | Durable |
/// |---|---|
/// | `SessionStarted` | true |
/// | `SessionEnded` | true |
/// | `SessionInterrupted` | true |
/// | `Progress` | false |
/// | `Warning` | false |
/// | `Recovery` | true |
/// | `Usage` | false |
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemEvent {
    /// A session has started running.
    SessionStarted { session_id: String },
    /// A session has ended; the `reason` discriminates why.
    SessionEnded { reason: SessionEndReason },
    /// A session was interrupted (e.g. by `Ctrl+C`); `reason` is the
    /// human-readable cause.
    SessionInterrupted { reason: String },
    /// Progress update for a long-running operation.
    Progress {
        message: String,
        step: usize,
        total: usize,
    },
    /// A warning surfaced from somewhere in the agent loop. `kind`
    /// classifies the source.
    Warning {
        kind: WarningKind,
        message: String,
        iteration: Option<usize>,
    },
    /// A recovery action was applied during the agent loop. Emitted
    /// for every L1 truncation, L3 fallback, L4 compact, and L5 reset
    /// so external observers can see *why* the session did not abort
    /// despite a tool/LLM error.
    ///
    /// `level_number`: 1 = Truncate, 2 = Retry, 3 = Fallback,
    /// 4 = Compact, 5 = Reset. `u32` is used instead of
    /// `crate::error_recovery::RecoveryLevel` to keep the public
    /// event wire format stable.
    ///
    /// `tool_name`: `Some(name)` for tool-specific recovery; the LLM
    /// sampling path uses the synthetic `Some("llm_sample")` so the
    /// field is never `None` (spec invariant: tool_name is
    /// `Some('llm_sample')` for LLM-only recovery).
    Recovery {
        level_number: u32,
        tool_name: Option<String>,
        message: String,
        iteration: Option<usize>,
    },
    /// Token usage rollup, emitted at the end of every LLM sampling
    /// pass. `cache_read_tokens` and `cache_creation_tokens` are `None`
    /// for providers that do not report cache metrics.
    Usage {
        input_tokens: usize,
        output_tokens: usize,
        cache_read_tokens: Option<usize>,
        cache_creation_tokens: Option<usize>,
    },
}

/// Classification of a [`SystemEvent::Warning`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    Guardian,
    Loop,
    TokenBudget,
    ContextCompaction,
    Hook,
    EditConflict,
}
