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
/// - `parent_depth` indicates the nesting depth (0 = top-level
///   session, 1 = direct subagent, 2 = nested subagent of a
///   subagent, …).
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

#[cfg(test)]
mod tests {
    use super::*;

    // -- AgentMeta::new ----------------------------------------------

    /// `AgentMeta::new` MUST build with
    /// all 3 fields propagated verbatim.
    #[test]
    fn new_propagates_all_three_fields() {
        let m = AgentMeta::new("parent-1", "child-1", 2);
        assert_eq!(m.parent_session_id, "parent-1");
        assert_eq!(m.child_session_id, "child-1");
        assert_eq!(m.parent_depth, 2);
    }

    /// `AgentMeta::new` MUST accept any
    /// `impl Into<String>` (both `&str`
    /// and `String`).
    #[test]
    fn new_accepts_string_and_str() {
        let m1 = AgentMeta::new(String::from("p"), String::from("c"), 0);
        let m2 = AgentMeta::new("p", "c", 0);
        assert_eq!(m1.parent_session_id, m2.parent_session_id);
        assert_eq!(m1.child_session_id, m2.child_session_id);
    }

    /// `AgentMeta::new` MUST accept
    /// `parent_depth = 0` (top-level
    /// session) and very large depths
    /// without panic.
    #[test]
    fn new_accepts_zero_and_large_depths() {
        let zero = AgentMeta::new("p", "c", 0);
        assert_eq!(zero.parent_depth, 0);
        let deep = AgentMeta::new("p", "c", usize::MAX);
        assert_eq!(deep.parent_depth, usize::MAX);
    }

    // -- JSON round-trip --------------------------------------------

    /// `AgentMeta` MUST round-trip every
    /// field verbatim through JSON.
    #[test]
    fn round_trips_through_json() {
        let m = AgentMeta::new("parent-1", "child-1", 3);
        let json = serde_json::to_string(&m).unwrap();
        let parsed: AgentMeta =
            serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(parsed, m);
    }

    // -- Trait surface ----------------------------------------------

    /// `AgentMeta` MUST support Clone +
    /// Debug + PartialEq + Eq (used in
    /// event ordering and dedup).
    #[test]
    fn supports_clone_debug_partial_eq_eq() {
        let m = AgentMeta::new("p", "c", 1);
        let _copy = m.clone();
        let _ = format!("{:?}", m);
        assert_eq!(m, m);
    }

    /// Two `AgentMeta` instances MUST be
    /// equal if and only if all 3 fields
    /// match exactly.
    #[test]
    fn equality_requires_all_three_fields_to_match() {
        let a = AgentMeta::new("p", "c", 1);
        let mut b = a.clone();
        // parent_depth differs
        b.parent_depth = 2;
        assert_ne!(a, b);
        // child_session_id differs
        let mut c = a.clone();
        c.child_session_id = "other".to_string();
        assert_ne!(a, c);
        // parent_session_id differs
        let mut d = a.clone();
        d.parent_session_id = "other".to_string();
        assert_ne!(a, d);
    }
}
