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
///   subagent, …).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMeta {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub parent_depth: usize,
    /// R29 (pi-subagents `invocation-config.ts:overridden`
    /// parity): the model the *caller* asked for, before
    /// descriptor-level overrides. `None` when the caller
    /// did not specify a model. UI surfaces should display
    /// "model · asked X" when this differs from
    /// [`effective_model`](Self::effective_model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_model: Option<String>,
    /// R29: the thinking effort the *caller* asked for.
    /// `None` when the caller did not specify a thinking
    /// level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_thinking: Option<String>,
    /// R29: the model the descriptor / frontmatter actually
    /// resolved to. When this differs from
    /// [`requested_model`](Self::requested_model), the
    /// caller asked for one and got another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_model: Option<String>,
    /// R29: the thinking effort the descriptor / frontmatter
    /// actually resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_thinking: Option<String>,
    /// R29: `true` iff a caller-specified model or thinking
    /// level was outranked by descriptor / frontmatter
    /// values. Lets UIs render a "asked X · got Y"
    /// affordance without diffing the request / effective
    /// fields manually.
    #[serde(default)]
    pub overrides_applied: bool,
    /// The registry name of the delegated peer (the `agent`
    /// field of the `task` tool call). `None` on traces
    /// emitted before the field existed or by delegators
    /// that resolve peers without a name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// The prompt the child was dispatched with. Set on the
    /// child's **first** forwarded event only, so a session
    /// router can persist it as the child's user turn without
    /// cloning the prompt on every event of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
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
            requested_model: None,
            requested_thinking: None,
            effective_model: None,
            effective_thinking: None,
            overrides_applied: false,
            agent_name: None,
            prompt: None,
        }
    }

    /// Builder: record the registry name of the delegated
    /// peer. Consumed by session routers that attribute the
    /// child trace (and by UIs labelling the sub-agent bubble).
    #[must_use]
    pub fn with_agent_name(mut self, name: Option<String>) -> Self {
        self.agent_name = name;
        self
    }

    /// Builder: carry the dispatch prompt. Attach to the first
    /// forwarded event of the child run only — see the field
    /// docs for why.
    #[must_use]
    pub fn with_prompt(mut self, prompt: Option<String>) -> Self {
        self.prompt = prompt;
        self
    }

    /// R29: builder: attach the requested / effective model
    /// and thinking level. When the two pairs differ,
    /// `overrides_applied` is set to `true` so UIs can
    /// render the "asked X" affordance.
    #[must_use]
    pub fn with_overrides(
        mut self,
        requested_model: Option<String>,
        requested_thinking: Option<String>,
        effective_model: Option<String>,
        effective_thinking: Option<String>,
    ) -> Self {
        let overrides_applied = requested_model != effective_model
            || requested_thinking != effective_thinking;
        self.requested_model = requested_model;
        self.requested_thinking = requested_thinking;
        self.effective_model = effective_model;
        self.effective_thinking = effective_thinking;
        self.overrides_applied = overrides_applied;
        self
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

    // R29 pi-subagents §6 parity: requested vs effective
    // model / thinking with overrides_applied.
    #[test]
    fn with_overrides_records_pairs_and_flags_difference() {
        let m = AgentMeta::new("p", "c", 0).with_overrides(
            Some("sonnet-4.6".to_string()),
            Some("low".to_string()),
            Some("opus-4.7".to_string()),
            Some("max".to_string()),
        );
        assert_eq!(m.requested_model.as_deref(), Some("sonnet-4.6"));
        assert_eq!(m.effective_model.as_deref(), Some("opus-4.7"));
        assert!(m.overrides_applied);
    }

    #[test]
    fn with_overrides_no_difference_keeps_flag_false() {
        let m = AgentMeta::new("p", "c", 0).with_overrides(
            Some("opus".to_string()),
            None,
            Some("opus".to_string()),
            None,
        );
        assert!(!m.overrides_applied);
    }

    #[test]
    fn new_default_has_no_overrides() {
        let m = AgentMeta::new("p", "c", 0);
        assert!(m.requested_model.is_none());
        assert!(m.effective_model.is_none());
        assert!(!m.overrides_applied);
    }
}
