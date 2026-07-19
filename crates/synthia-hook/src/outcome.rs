//! `HookOutcome` 3-state + 10 typed hook events.
//!
//! PR-4.1 introduces the unified hook outcome enum and the 10 typed
//! event payloads that the new `Hook` trait operates on.
//!
//! See `specs/hook-system-unification/spec.md`
//! (Requirements: "HookOutcome 3-state" + "10 typed hook events").

use serde::{Deserialize, Serialize};

// ── HookOutcome 3-state ────────────────────────────────────

/// The outcome returned by every [`crate::Hook::on_event`] call.
///
/// Replaces the previous 2-state `ToolAction` / `HookResult` with a
/// 3-state model that distinguishes "deny" (hard block) from
/// "forward to main agent" (soft redirect).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HookOutcome {
    /// Proceed with the original event flow.
    #[default]
    Allow,
    /// Abort the current step. The `reason` is propagated to the
    /// caller. The system synthesizes a `PreMessageDrop` event
    /// before the actual drop.
    Deny {
        /// Why the hook denied the action.
        reason: String,
    },
    /// Route the event to the main agent queue without blocking
    /// the subagent that triggered the hook.
    ForwardToMainAgent {
        /// Hint for the main agent about why the event was forwarded.
        hint: String,
    },
}

impl HookOutcome {
    /// Whether this outcome allows the event to proceed.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Whether this outcome denies the event.
    #[must_use]
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }
}

// ── 10 typed hook events ───────────────────────────────────

/// The 10 typed hook events for the unified `Hook` trait.
///
/// Each variant carries a strongly-typed payload struct (NOT
/// `serde_json::Value`), eliminating the need for downcasting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    /// Session has started.
    SessionStart(SessionStartPayload),
    /// Session has ended.
    SessionEnd(SessionEndPayload),
    /// User submitted a prompt.
    UserPromptSubmit(UserPromptSubmitPayload),
    /// About to use a tool (may be denied).
    PreToolUse(PreToolUsePayload),
    /// Tool has been used.
    PostToolUse(PostToolUsePayload),
    /// About to generate a response.
    PreResponse(PreResponsePayload),
    /// Response has been generated.
    PostResponse(PostResponsePayload),
    /// About to compact the context.
    PreCompact(PreCompactPayload),
    /// Context has been compacted.
    PostCompact(PostCompactPayload),
    /// A message is about to be dropped (Synthia 独有).
    PreMessageDrop(PreMessageDropPayload),
}

impl HookEvent {
    /// Returns the event name as a static string (for tracing/metrics).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::SessionStart(_) => "SessionStart",
            Self::SessionEnd(_) => "SessionEnd",
            Self::UserPromptSubmit(_) => "UserPromptSubmit",
            Self::PreToolUse(_) => "PreToolUse",
            Self::PostToolUse(_) => "PostToolUse",
            Self::PreResponse(_) => "PreResponse",
            Self::PostResponse(_) => "PostResponse",
            Self::PreCompact(_) => "PreCompact",
            Self::PostCompact(_) => "PostCompact",
            Self::PreMessageDrop(_) => "PreMessageDrop",
        }
    }
}

// ── Event payload structs ───────────────────────────────────

/// Payload for `SessionStart`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartPayload {
    /// Session identifier.
    pub session_id: String,
}

/// Payload for `SessionEnd`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndPayload {
    /// Session identifier.
    pub session_id: String,
    /// Why the session ended.
    pub reason: SessionEndReason,
}

/// Reason a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionEndReason {
    /// Normal completion.
    Completed,
    /// User cancelled.
    Cancelled,
    /// Error occurred.
    Error,
    /// Evicted by runtime.
    Evicted,
}

/// Payload for `UserPromptSubmit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitPayload {
    /// Session identifier.
    pub session_id: String,
    /// The prompt text (may be truncated for privacy).
    pub prompt_summary: String,
}

/// Payload for `PreToolUse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreToolUsePayload {
    /// Session identifier.
    pub session_id: String,
    /// Name of the tool about to be used.
    pub tool_name: String,
    /// Tool call input (JSON).
    pub input: serde_json::Value,
}

/// Payload for `PostToolUse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostToolUsePayload {
    /// Session identifier.
    pub session_id: String,
    /// Name of the tool that was used.
    pub tool_name: String,
    /// Tool call input (JSON).
    pub input: serde_json::Value,
    /// Tool execution result (JSON).
    pub output: serde_json::Value,
}

/// Payload for `PreResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreResponsePayload {
    /// Session identifier.
    pub session_id: String,
}

/// Payload for `PostResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostResponsePayload {
    /// Session identifier.
    pub session_id: String,
    /// Summary of the response.
    pub response_summary: String,
}

/// Payload for `PreCompact`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCompactPayload {
    /// Session identifier.
    pub session_id: String,
    /// Current token count before compaction.
    pub token_count: usize,
}

/// Payload for `PostCompact`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostCompactPayload {
    /// Session identifier.
    pub session_id: String,
    /// Token count after compaction.
    pub token_count: usize,
}

/// Payload for `PreMessageDrop` (Synthia 独有).
///
/// Fired before a message is actually dropped (timeout, cancellation,
/// or tool failure). This gives hooks a chance to observe or log the
/// drop before it happens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreMessageDropPayload {
    /// Session identifier.
    pub session_id: String,
    /// Why the message is being dropped.
    pub reason: DropReason,
}

/// Why a message is being dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DropReason {
    /// Operation timed out.
    Timeout,
    /// User cancelled the operation.
    Cancelled,
    /// Tool execution failed.
    ToolFailure,
    /// Hook denied the operation (triggered by `HookOutcome::Deny`).
    HookDenied,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_outcome_default_is_allow() {
        assert_eq!(HookOutcome::default(), HookOutcome::Allow);
        assert!(HookOutcome::default().is_allowed());
    }

    #[test]
    fn hook_outcome_deny_is_not_allowed() {
        let deny = HookOutcome::Deny {
            reason: "test".into(),
        };
        assert!(!deny.is_allowed());
        assert!(deny.is_denied());
    }

    #[test]
    fn hook_outcome_forward_is_not_denied() {
        let fwd = HookOutcome::ForwardToMainAgent {
            hint: "check".into(),
        };
        assert!(!fwd.is_allowed());
        assert!(!fwd.is_denied());
    }

    #[test]
    fn hook_event_name_covers_all_10() {
        let events = [
            HookEvent::SessionStart(SessionStartPayload {
                session_id: "s".into(),
            }),
            HookEvent::SessionEnd(SessionEndPayload {
                session_id: "s".into(),
                reason: SessionEndReason::Completed,
            }),
            HookEvent::UserPromptSubmit(UserPromptSubmitPayload {
                session_id: "s".into(),
                prompt_summary: String::new(),
            }),
            HookEvent::PreToolUse(PreToolUsePayload {
                session_id: "s".into(),
                tool_name: "t".into(),
                input: serde_json::Value::Null,
            }),
            HookEvent::PostToolUse(PostToolUsePayload {
                session_id: "s".into(),
                tool_name: "t".into(),
                input: serde_json::Value::Null,
                output: serde_json::Value::Null,
            }),
            HookEvent::PreResponse(PreResponsePayload {
                session_id: "s".into(),
            }),
            HookEvent::PostResponse(PostResponsePayload {
                session_id: "s".into(),
                response_summary: String::new(),
            }),
            HookEvent::PreCompact(PreCompactPayload {
                session_id: "s".into(),
                token_count: 0,
            }),
            HookEvent::PostCompact(PostCompactPayload {
                session_id: "s".into(),
                token_count: 0,
            }),
            HookEvent::PreMessageDrop(PreMessageDropPayload {
                session_id: "s".into(),
                reason: DropReason::Timeout,
            }),
        ];
        let names: Vec<&str> = events.iter().map(|e| e.name()).collect();
        assert_eq!(names.len(), 10);
        // Exhaustive match compile test: if a new variant is added
        // without updating `name()`, this will fail to compile.
    }

    #[test]
    fn hook_outcome_serde_roundtrip() {
        let outcomes = [
            HookOutcome::Allow,
            HookOutcome::Deny {
                reason: "bad".into(),
            },
            HookOutcome::ForwardToMainAgent {
                hint: "hint".into(),
            },
        ];
        for outcome in &outcomes {
            let json = serde_json::to_string(outcome).unwrap();
            let parsed: HookOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(*outcome, parsed);
        }
    }

    #[test]
    fn hook_event_serde_roundtrip() {
        let event = HookEvent::PreToolUse(PreToolUsePayload {
            session_id: "test".into(),
            tool_name: "bash".into(),
            input: serde_json::json!({"cmd": "ls"}),
        });
        let json = serde_json::to_string(&event).unwrap();
        let parsed: HookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.name(), parsed.name());
    }
}
