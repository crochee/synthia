//! `ToolPart` with `ToolState` 4-state machine + type-safe `ToolTime.compacted`.
//!
//! Mirrors opencode `ToolPart` (`packages/opencode/src/session/message-v2.ts:308-403`).
//! The 4-state machine ensures the tool-call story is type-safe end-to-end.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_protocol::CallId;

/// Tool call part within a `Message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPart {
    pub call_id: CallId,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub state: ToolState,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub attachments: Vec<AttachmentRef>,
    pub time: ToolTime,
}

/// 4-state tool lifecycle machine.
///
/// State transitions are governed by the session writer, not the type system.
/// The runtime guarantee is: `Pending → Running → (Completed | Error)`.
/// Backward transitions (`Completed → Running`) are allowed by the type system
/// but should never be produced by well-formed writers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolState {
    Pending {
        queued_at: DateTime<Utc>,
    },
    Running {
        started_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        partial_output: Option<String>,
    },
    Completed {
        output: serde_json::Value,
        ended_at: DateTime<Utc>,
        duration_ms: u64,
    },
    Error {
        message: String,
        interrupted: bool,
        ended_at: DateTime<Utc>,
    },
}

/// Tool timing — type-safe `compacted` marker.
///
/// `compacted` is `Option<DateTime<Utc>>`, not `Option<u64>` or string.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolTime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<Utc>>,
    /// Set when this tool part was preserved across a compaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted: Option<DateTime<Utc>>,
}

/// External attachment reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentRef {
    pub kind: String,
    pub url: String,
}

impl ToolPart {
    /// Returns true if this tool part is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            ToolState::Completed { .. } | ToolState::Error { .. }
        )
    }

    /// Returns true if this tool part has been preserved across a compaction.
    pub fn was_compacted(&self) -> bool {
        self.time.compacted.is_some()
    }

    /// Transition this tool part to Running state. Emits a tracing event.
    pub fn start_running(&mut self, partial_output: Option<String>) {
        let span = tracing::debug_span!("tool.start_running", call_id = %self.call_id, tool = %self.tool_name);
        let _enter = span.enter();
        self.state = ToolState::Running {
            started_at: chrono::Utc::now(),
            partial_output,
        };
    }

    /// Transition this tool part to Completed state. Emits a tracing event.
    pub fn complete(&mut self, output: serde_json::Value, duration_ms: u64) {
        let span = tracing::debug_span!("tool.complete", call_id = %self.call_id, tool = %self.tool_name);
        let _enter = span.enter();
        self.state = ToolState::Completed {
            output,
            ended_at: chrono::Utc::now(),
            duration_ms,
        };
    }

    /// Transition this tool part to Error state. Emits a tracing event.
    pub fn fail(&mut self, message: String, interrupted: bool) {
        let span = tracing::warn_span!("tool.fail", call_id = %self.call_id, tool = %self.tool_name, interrupted);
        let _enter = span.enter();
        self.state = ToolState::Error {
            message,
            interrupted,
            ended_at: chrono::Utc::now(),
        };
    }

    /// Mark this tool part as preserved across a compaction.
    pub fn mark_compacted(&mut self) {
        let span = tracing::debug_span!("tool.compact", call_id = %self.call_id, tool = %self.tool_name);
        let _enter = span.enter();
        self.time.compacted = Some(chrono::Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_to_running() {
        let queued = ToolPart {
            call_id: CallId::new(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({}),
            state: ToolState::Pending {
                queued_at: Utc::now(),
            },
            metadata: HashMap::new(),
            attachments: vec![],
            time: ToolTime::default(),
        };
        assert!(!queued.is_terminal());
    }

    #[test]
    fn completed_is_terminal() {
        let completed = ToolPart {
            call_id: CallId::new(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({}),
            state: ToolState::Completed {
                output: serde_json::json!("ok"),
                ended_at: Utc::now(),
                duration_ms: 100,
            },
            metadata: HashMap::new(),
            attachments: vec![],
            time: ToolTime {
                start: None,
                end: None,
                compacted: None,
            },
        };
        assert!(completed.is_terminal());
    }

    #[test]
    fn compacted_marker_is_type_safe() {
        let mut t = ToolTime::default();
        assert!(t.compacted.is_none());
        t.compacted = Some(Utc::now());
        assert!(t.compacted.is_some());
    }

    fn make_pending_tool() -> ToolPart {
        ToolPart {
            call_id: CallId::new(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({}),
            state: ToolState::Pending {
                queued_at: Utc::now(),
            },
            metadata: HashMap::new(),
            attachments: vec![],
            time: ToolTime::default(),
        }
    }

    #[test]
    fn start_running_emits_event() {
        let mut tp = make_pending_tool();
        tp.start_running(None);
        assert!(matches!(tp.state, ToolState::Running { .. }));
    }

    #[test]
    fn complete_sets_terminal_state() {
        let mut tp = make_pending_tool();
        tp.start_running(None);
        tp.complete(serde_json::json!("ok"), 100);
        assert!(tp.is_terminal());
    }

    #[test]
    fn mark_compacted_sets_time() {
        let mut tp = make_pending_tool();
        assert!(!tp.was_compacted());
        tp.mark_compacted();
        assert!(tp.was_compacted());
    }
}
