//! [`AgentMeta`] struct: per-subagent metadata carried by every
//! [`AgentEvent::Agent`](super::AgentEvent::Agent) variant.

use serde::{Deserialize, Serialize};

/// Identifying metadata for a subagent trace.
///
/// Carried as the first argument of
/// [`AgentEvent::Agent`](super::AgentEvent::Agent).
///
/// - `parent_session_id` identifies the spawning parent session.
/// - `child_session_id` identifies the child session that produced
///   the inner event.
/// - `parent_depth` indicates the nesting depth (0 = top-level session,
///   1 = direct subagent, 2 = nested subagent of a subagent, …).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMeta {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub parent_depth: usize,
}

impl AgentMeta {
    /// Construct a new [`AgentMeta`].
    pub fn new(
        parent_session_id: impl Into<String>,
        child_session_id: impl Into<String>,
        parent_depth: usize,
    ) -> Self {
        Self {
            parent_session_id: parent_session_id.into(),
            child_session_id: child_session_id.into(),
            parent_depth,
        }
    }
}
