//! Hook phase definitions with ordering

use std::fmt;

/// Hook phases in order of execution.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum HookPhase {
    Session,
    Agent,
    Mode,
    LLM,
    Step,
    Turn,
    Tool,
    Context,
    Team,
}

impl HookPhase {
    /// Returns phases in execution order.
    pub fn execution_order() -> Vec<HookPhase> {
        vec![
            HookPhase::Session,
            HookPhase::Agent,
            HookPhase::Mode,
            HookPhase::LLM,
            HookPhase::Step,
            HookPhase::Turn,
            HookPhase::Tool,
            HookPhase::Context,
            HookPhase::Team,
        ]
    }

    /// Returns true if this is a tool-related phase.
    pub fn is_tool_phase(self) -> bool {
        matches!(self, HookPhase::Tool)
    }

    /// Returns true if this is a step-related phase.
    pub fn is_step_phase(self) -> bool {
        matches!(self, HookPhase::Step)
    }
}

impl fmt::Display for HookPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookPhase::Session => write!(f, "Session"),
            HookPhase::Agent => write!(f, "Agent"),
            HookPhase::Mode => write!(f, "Mode"),
            HookPhase::LLM => write!(f, "LLM"),
            HookPhase::Step => write!(f, "Step"),
            HookPhase::Turn => write!(f, "Turn"),
            HookPhase::Tool => write!(f, "Tool"),
            HookPhase::Context => write!(f, "Context"),
            HookPhase::Team => write!(f, "Team"),
        }
    }
}

/// Trait for phases with ordering.
pub trait PhaseOrder {
    fn phases() -> Vec<HookPhase>;
}

impl PhaseOrder for HookPhase {
    fn phases() -> Vec<HookPhase> {
        Self::execution_order()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_phase_order() {
        let order = HookPhase::execution_order();
        assert_eq!(order.len(), 9);
        assert_eq!(order[0], HookPhase::Session);
        assert_eq!(order[1], HookPhase::Agent);
        assert_eq!(order[2], HookPhase::Mode);
        assert_eq!(order[3], HookPhase::LLM);
        assert_eq!(order[4], HookPhase::Step);
        assert_eq!(order[5], HookPhase::Turn);
        assert_eq!(order[6], HookPhase::Tool);
        assert_eq!(order[7], HookPhase::Context);
        assert_eq!(order[8], HookPhase::Team);
    }

    #[test]
    fn test_hook_phase_ord() {
        assert!(HookPhase::Session < HookPhase::Agent);
        assert!(HookPhase::Agent < HookPhase::Mode);
        assert!(HookPhase::Mode < HookPhase::LLM);
        assert!(HookPhase::LLM < HookPhase::Step);
        assert!(HookPhase::Step < HookPhase::Turn);
        assert!(HookPhase::Turn < HookPhase::Tool);
        assert!(HookPhase::Tool < HookPhase::Context);
        assert!(HookPhase::Context < HookPhase::Team);
    }

    #[test]
    fn test_is_tool_phase() {
        assert!(!HookPhase::Session.is_tool_phase());
        assert!(!HookPhase::Agent.is_tool_phase());
        assert!(!HookPhase::Mode.is_tool_phase());
        assert!(!HookPhase::LLM.is_tool_phase());
        assert!(!HookPhase::Step.is_tool_phase());
        assert!(!HookPhase::Turn.is_tool_phase());
        assert!(HookPhase::Tool.is_tool_phase());
        assert!(!HookPhase::Context.is_tool_phase());
        assert!(!HookPhase::Team.is_tool_phase());
    }

    #[test]
    fn test_is_step_phase() {
        assert!(!HookPhase::Session.is_step_phase());
        assert!(!HookPhase::Agent.is_step_phase());
        assert!(!HookPhase::Mode.is_step_phase());
        assert!(!HookPhase::LLM.is_step_phase());
        assert!(HookPhase::Step.is_step_phase());
        assert!(!HookPhase::Turn.is_step_phase());
        assert!(!HookPhase::Tool.is_step_phase());
        assert!(!HookPhase::Context.is_step_phase());
        assert!(!HookPhase::Team.is_step_phase());
    }

    #[test]
    fn test_hook_phase_display() {
        assert_eq!(HookPhase::Session.to_string(), "Session");
        assert_eq!(HookPhase::Agent.to_string(), "Agent");
        assert_eq!(HookPhase::Mode.to_string(), "Mode");
        assert_eq!(HookPhase::LLM.to_string(), "LLM");
        assert_eq!(HookPhase::Step.to_string(), "Step");
        assert_eq!(HookPhase::Turn.to_string(), "Turn");
        assert_eq!(HookPhase::Tool.to_string(), "Tool");
        assert_eq!(HookPhase::Context.to_string(), "Context");
        assert_eq!(HookPhase::Team.to_string(), "Team");
    }

    #[test]
    fn test_phase_order_trait() {
        let phases = <HookPhase as PhaseOrder>::phases();
        assert_eq!(phases, HookPhase::execution_order());
    }
}
