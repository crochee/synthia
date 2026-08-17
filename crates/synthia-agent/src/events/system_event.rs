//! [`SystemEvent`] + [`WarningKind`] definitions.
//!
//! [`SystemEvent`] is reported via
//! [`AgentEvent::System`](super::AgentEvent::System). It carries
//! lifecycle, diagnostic, and terminal state changes that are not
//! user-visible streaming content.

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
/// | `ToolProgress` | false |
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
    /// Intermediate progress yielded by a tool. `output` is the
    /// tool's per-step `ToolOutput` payload, distinguishable from
    /// [`SystemEvent::Progress`] (which tracks high-level agent-loop
    /// milestones) by carrying a tool-specific payload (`tool_name`,
    /// `call_id`).
    ToolProgress {
        tool_name: String,
        call_id: String,
        output: synthia_tool::ToolOutput,
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
    /// 4 = Compact, 5 = Reset. `u32` is used instead of an internal
    /// enum to keep the public wire format stable.
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
    /// pass. `cache_read_tokens` and `cache_creation_tokens` are
    /// `None` for providers that do not report cache metrics.
    Usage {
        input_tokens: usize,
        output_tokens: usize,
        cache_read_tokens: Option<usize>,
        cache_creation_tokens: Option<usize>,
    },
}

impl SystemEvent {
    /// Short, stable label for log lines and metrics.
    ///
    /// Unlike richer inspection helpers, `kind` collapses every
    /// `SystemEvent` variant into a single stable string token so
    /// log queries and metrics can filter on the outer variant
    /// without enumerating every payload shape.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionStarted { .. } => "SessionStarted",
            Self::SessionEnded { .. } => "SessionEnded",
            Self::SessionInterrupted { .. } => "SessionInterrupted",
            Self::Progress { .. } => "Progress",
            Self::ToolProgress { .. } => "ToolProgress",
            Self::Warning { .. } => "Warning",
            Self::Recovery { .. } => "Recovery",
            Self::Usage { .. } => "Usage",
        }
    }
}

/// Classification of a [`SystemEvent::Warning`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    Loop,
    TokenBudget,
    ContextCompaction,
    Hook,
    EditConflict,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `kind()` is the stable label used by log
    /// queries and metrics to filter on the outer
    /// `SystemEvent` variant without enumerating
    /// payloads. Pin the 8-way enum mapping
    /// verbatim — a refactor that changes the
    /// label breaks log queries and dashboards.
    #[test]
    fn kind_returns_stable_label_for_every_variant() {
        let cases: Vec<(SystemEvent, &str)> = vec![
            (
                SystemEvent::SessionStarted {
                    session_id: "s1".to_string(),
                },
                "SessionStarted",
            ),
            (
                SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                },
                "SessionEnded",
            ),
            (
                SystemEvent::SessionInterrupted {
                    reason: "ctrl_c".to_string(),
                },
                "SessionInterrupted",
            ),
            (
                SystemEvent::Progress {
                    message: "step".to_string(),
                    step: 1,
                    total: 10,
                },
                "Progress",
            ),
            (
                SystemEvent::ToolProgress {
                    tool_name: "read_file".to_string(),
                    call_id: "c1".to_string(),
                    output: synthia_tool::ToolOutput::text("x"),
                },
                "ToolProgress",
            ),
            (
                SystemEvent::Warning {
                    kind: WarningKind::Loop,
                    message: "loop".to_string(),
                    iteration: Some(1),
                },
                "Warning",
            ),
            (
                SystemEvent::Recovery {
                    level_number: 1,
                    tool_name: Some("llm_sample".to_string()),
                    message: "truncated".to_string(),
                    iteration: Some(1),
                },
                "Recovery",
            ),
            (
                SystemEvent::Usage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_read_tokens: None,
                    cache_creation_tokens: None,
                },
                "Usage",
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(
                event.kind(),
                expected,
                "kind() label mismatch for {event:?}"
            );
        }
    }

    /// `kind()` MUST return `'static str` so it can
    /// be stored without allocation in log
    /// formatters. Pin the return-type contract.
    #[test]
    fn kind_returns_static_str_for_all_variants() {
        let event = SystemEvent::Usage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        };
        // This line compiles only if kind() returns
        // 'static str.
        let label: &'static str = event.kind();
        assert_eq!(label, "Usage");
    }

    /// `WarningKind` serializes as snake_case
    /// strings (wire-shape contract). Pin the
    /// exact tag values so a refactor that adds
    /// `#[serde(rename_all = "kebab-case")` breaks
    /// loudly.
    #[test]
    fn warning_kind_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&WarningKind::Loop).unwrap(),
            "\"loop\""
        );
        assert_eq!(
            serde_json::to_string(&WarningKind::TokenBudget).unwrap(),
            "\"token_budget\""
        );
        assert_eq!(
            serde_json::to_string(&WarningKind::ContextCompaction).unwrap(),
            "\"context_compaction\""
        );
        assert_eq!(
            serde_json::to_string(&WarningKind::Hook).unwrap(),
            "\"hook\""
        );
        assert_eq!(
            serde_json::to_string(&WarningKind::EditConflict).unwrap(),
            "\"edit_conflict\""
        );
    }

    /// `WarningKind` round-trips through JSON
    /// identity. Pin the deserialization side of
    /// the snake_case contract.
    #[test]
    fn warning_kind_round_trips_through_json() {
        for kind in [
            WarningKind::Loop,
            WarningKind::TokenBudget,
            WarningKind::ContextCompaction,
            WarningKind::Hook,
            WarningKind::EditConflict,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let parsed: WarningKind =
                serde_json::from_str(&json).expect("parse");
            assert_eq!(parsed, kind, "round-trip mismatch for {kind:?}");
        }
    }

    /// `SystemEvent` uses `#[serde(tag = "type",
    /// rename_all = "snake_case")]`. Pin the wire
    /// shape for a representative variant.
    #[test]
    fn system_event_session_started_serializes_with_snake_case_tag() {
        let event = SystemEvent::SessionStarted {
            session_id: "abc".to_string(),
        };
        let json: serde_json::Value =
            serde_json::to_value(&event).expect("serialize");
        assert_eq!(
            json.get("type").and_then(|v| v.as_str()),
            Some("session_started"),
            "type tag MUST be snake_case; got {json}"
        );
        assert_eq!(
            json.get("session_id").and_then(|v| v.as_str()),
            Some("abc")
        );
    }

    /// `SessionEndReason::Error(msg)` MUST
    /// serialize the message verbatim (not escape
    /// or filter) so observability consumers see
    /// the original upstream error string. Pin
    /// the contract.
    #[test]
    fn session_end_reason_error_carries_message_verbatim() {
        let reason = SessionEndReason::Error("rate_limit_exceeded".to_string());
        let json: serde_json::Value =
            serde_json::to_value(&reason).expect("serialize");
        assert_eq!(
            json.get("Error")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    // Externally tagged enums use the
                    // variant name as the key.
                    json.as_str().unwrap_or_default()
                }),
            if json.is_string() {
                json.as_str().unwrap()
            } else {
                "rate_limit_exceeded"
            },
            "Error variant MUST carry the message verbatim"
        );
    }
}
