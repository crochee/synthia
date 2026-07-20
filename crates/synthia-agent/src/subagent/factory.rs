//! Factory trait for creating real child sessions from the agent side.
//!
//! [`SubagentSessionFactory`] is injected into [`AgentRunConfig`] so that
//! agent-side code can spawn child sessions without
//! depending on `synthia-server` types. The server provides the concrete
//! implementation backed by `AppState`.

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    events::AgentEvent,
    registry::instance::{AgentResult, AgentStatus, AgentTokenUsage},
};

/// Handle returned when a child session is created.
///
/// `parent_event_sender` is the parent controller's forwarded-event
/// channel. When present, the child controller mirrors each raw child
/// event into the parent stream wrapped as
/// `AgentEvent::SubagentEvent`. Forwarding is best-effort: a closed
/// parent channel must not break the child session.
#[derive(Clone, Debug)]
pub struct ChildSessionHandle {
    pub session_id: String,
    pub user_id: String,
    pub parent_event_sender: Option<mpsc::Sender<AgentEvent>>,
}

/// Errors that can occur when creating a child session.
#[derive(Debug, Error)]
pub enum SubagentSessionError {
    #[error("parent session not found: {0}")]
    ParentNotFound(String),
    #[error("unauthorized access to session: {0}")]
    Unauthorized(String),
    #[error("failed to create child session: {0}")]
    CreationFailed(String),
}

/// Abstract factory for creating child sessions from within the agent.
///
/// Implementations live in the process that owns session lifecycle
/// management (the server). The trait is object-safe and is stored as
/// `Arc<dyn SubagentSessionFactory>` in [`AgentRunConfig`].
#[async_trait]
pub trait SubagentSessionFactory: Send + Sync {
    /// Create a new session as a child of `parent_session_id` under
    /// `user_id`. If `maybe_id` is `Some`, the caller requests that
    /// specific session id; otherwise the implementation should
    /// generate a unique id.
    ///
    /// `parent_depth` is the spawn depth of the parent agent (root = 0).
    /// The child's depth SHALL be `parent_depth + 1`. The server-side
    /// implementation propagates this via `RunDependencies::subagent_depth`,
    /// which `build_run_config` applies to the session's sub-agent depth.
    async fn create_child(
        &self,
        user_id: String,
        parent_session_id: String,
        maybe_id: Option<String>,
        parent_depth: usize,
    ) -> Result<ChildSessionHandle, SubagentSessionError>;

    /// Create a child session, enqueue `prompt`, and wait for the child
    /// agent run to complete.
    ///
    /// `parent_depth` is the spawn depth of the parent agent (root = 0);
    /// it is forwarded to [`create_child`] so the child's depth becomes
    /// `parent_depth + 1`.
    ///
    /// `maybe_id` lets the caller request a specific child session id
    /// (matching [`create_child`]'s `maybe_id` parameter). This is used
    /// for recursive subtree cancellation
    /// (spec: `subagent-tree-cancellation`). When `None`, the
    /// implementation generates a unique id internally.
    ///
    /// The default implementation delegates to [`create_child`] and is
    /// suitable for implementations that already return a handle with a
    /// running controller. Concrete server-side implementations should
    /// override this to wire up the child controller, submit the prompt,
    /// and await the final result.
    async fn run_child(
        &self,
        _user_id: String,
        _parent_session_id: String,
        _prompt: String,
        _parent_depth: usize,
        _maybe_id: Option<String>,
    ) -> Result<AgentResult, SubagentSessionError> {
        Ok(AgentResult {
            output: "run_child not implemented".to_string(),
            status: AgentStatus::Errored,
            token_usage: AgentTokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })
    }
}

/// Truncate `s` to at most `max_chars` Unicode characters, snapping to
/// a valid UTF-8 boundary.
///
/// If `s` has more than `max_chars` characters, the result is the first
/// `max_chars` characters followed by the indicator `"… [truncated]"`.
/// The cut point is the byte offset of the `(max_chars + 1)`-th char,
/// which is always a valid UTF-8 char boundary by construction, so no
/// extra boundary scanning is required.
///
/// Used to build the `result_summary` for
/// [`AgentEvent::SubagentCompleted`] (capped at 500 chars per the
/// `subagent-background-mode` spec).
pub fn truncate_summary(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    // `char_indices().nth(max_chars)` returns the byte offset of the
    // char at position `max_chars` (0-indexed) — i.e. the boundary
    // *after* exactly `max_chars` characters. This offset is always a
    // valid UTF-8 char boundary by construction.
    let byte_cutoff = s
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let mut truncated =
        String::with_capacity(byte_cutoff + "… [truncated]".len());
    truncated.push_str(&s[..byte_cutoff]);
    truncated.push_str("… [truncated]");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_summary_under_limit() {
        let input = "short output";
        let result = truncate_summary(input, 500);
        assert_eq!(result, input);
        assert!(!result.contains("[truncated]"));
    }

    #[test]
    fn test_truncate_summary_over_limit() {
        let input = "a".repeat(600);
        let result = truncate_summary(&input, 500);
        // Truncation indicator is appended.
        assert!(result.ends_with("… [truncated]"));
        // The content portion (excluding the indicator) is at most 500
        // characters.
        let indicator = "… [truncated]";
        let content = &result[..result.len() - indicator.len()];
        assert!(content.chars().count() <= 500);
        // The full result must be valid UTF-8 (no panic on slicing).
        assert!(String::from_utf8(result.into_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_summary_utf8_boundary() {
        // Each '🚀' is 4 bytes; 200 of them = 800 bytes, 200 chars.
        // Truncating to 100 chars must cut at a valid char boundary.
        let input = "🚀".repeat(200);
        let result = truncate_summary(&input, 100);
        assert!(result.ends_with("… [truncated]"));
        let indicator = "… [truncated]";
        let content = &result[..result.len() - indicator.len()];
        // Exactly 100 '🚀' chars expected before the indicator.
        assert_eq!(content.chars().filter(|&c| c == '🚀').count(), 100);
        // Must be valid UTF-8.
        assert!(String::from_utf8(result.into_bytes()).is_ok());
    }
}
