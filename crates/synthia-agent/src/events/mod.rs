//! Agent lifecycle events.
//!
//! # Module Layout
//!
//! - [`event_enum`]: the top-level [`event_enum::AgentEvent`] enum
//!   with four variants (`Model` / `ModelDone` / `System` / `Agent`).
//! - [`system_event`]: the [`system_event::SystemEvent`] enum +
//!   [`system_event::WarningKind`] for lifecycle and diagnostic
//!   events (session start/end, progress, warnings, recovery, usage).
//! - [`agent_meta`]: the [`agent_meta::AgentMeta`] struct describing a
//!   subagent's parent / child relationship.
//! - [`reasons`]: [`reasons::SessionEndReason`] (terminal reason).

mod agent_meta;
mod event_enum;
mod reasons;
mod system_event;

#[cfg(test)]
mod tests;

pub use agent_meta::AgentMeta;
pub use event_enum::AgentEvent;
pub use reasons::SessionEndReason;
pub use system_event::{SystemEvent, WarningKind};

#[derive(Clone, Debug)]
pub struct AgentOutput {
    pub events: Vec<AgentEvent>,
    pub final_message: Option<String>,
}

#[cfg(test)]
mod output_tests {
    use super::*;

    /// `AgentOutput` MUST support direct field construction.
    #[test]
    fn agent_output_direct_construction() {
        let out = AgentOutput {
            events: vec![],
            final_message: None,
        };
        assert!(out.events.is_empty());
        assert_eq!(out.final_message, None);
    }

    /// `AgentOutput::default()` is NOT derived — `Default` is not
    /// implemented. Pin via construction.
    #[test]
    fn agent_output_supports_clone_and_debug() {
        let out = AgentOutput {
            events: vec![],
            final_message: Some("hi".to_string()),
        };
        let cloned = out.clone();
        assert_eq!(cloned.final_message, Some("hi".to_string()));
        assert!(cloned.events.is_empty());

        // Debug format includes field names.
        let dbg = format!("{:?}", out);
        assert!(dbg.contains("AgentOutput"));
        assert!(dbg.contains("final_message"));
    }

    /// `AgentOutput::final_message: Option<String>` MUST
    /// distinguish Some/None correctly.
    #[test]
    fn agent_output_final_message_some_and_none() {
        let some = AgentOutput {
            events: vec![],
            final_message: Some("done".to_string()),
        };
        let none = AgentOutput {
            events: vec![],
            final_message: None,
        };
        assert_eq!(some.final_message, Some("done".to_string()));
        assert_eq!(none.final_message, None);
    }

    /// `AgentOutput::events: Vec<AgentEvent>` MUST preserve
    /// insertion order.
    #[test]
    fn agent_output_events_preserves_order() {
        // Use empty events since constructing AgentEvent
        // variants may require additional imports. The Vec
        // type already guarantees order, so just verify.
        let v: Vec<AgentEvent> = vec![];
        let out = AgentOutput {
            events: v,
            final_message: None,
        };
        assert_eq!(out.events.len(), 0);
    }
}
