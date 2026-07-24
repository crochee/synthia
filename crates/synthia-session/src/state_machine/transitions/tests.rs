use super::*;
use crate::types::SessionState;

// --- Transition validation tests ---

#[test]
fn test_valid_transitions_from_initializing() {
    assert!(is_valid_transition(
        SessionState::Initializing,
        SessionState::WaitingForInput
    ));
}

#[test]
fn test_valid_transitions_from_waiting_for_input() {
    assert!(is_valid_transition(
        SessionState::WaitingForInput,
        SessionState::LlmCalling
    ));
    assert!(is_valid_transition(
        SessionState::WaitingForInput,
        SessionState::Paused
    ));
    assert!(is_valid_transition(
        SessionState::WaitingForInput,
        SessionState::Cancelled
    ));
}

#[test]
fn test_valid_transitions_from_llm_calling() {
    assert!(is_valid_transition(
        SessionState::LlmCalling,
        SessionState::ToolScheduling
    ));
    assert!(is_valid_transition(
        SessionState::LlmCalling,
        SessionState::WaitingForInput
    ));
    assert!(is_valid_transition(
        SessionState::LlmCalling,
        SessionState::Completed
    ));
    assert!(is_valid_transition(
        SessionState::LlmCalling,
        SessionState::Cancelled
    ));
    assert!(is_valid_transition(
        SessionState::LlmCalling,
        SessionState::Error
    ));
}

#[test]
fn test_valid_transitions_from_tool_scheduling() {
    assert!(is_valid_transition(
        SessionState::ToolScheduling,
        SessionState::WaitingForInput
    ));
    assert!(is_valid_transition(
        SessionState::ToolScheduling,
        SessionState::Cancelled
    ));
    assert!(is_valid_transition(
        SessionState::ToolScheduling,
        SessionState::Error
    ));
}

#[test]
fn test_compacting_from_any_state() {
    // Compacting can be entered from any state
    let states = vec![
        SessionState::Initializing,
        SessionState::WaitingForInput,
        SessionState::LlmCalling,
        SessionState::ToolScheduling,
        SessionState::WaitingForApproval,
        SessionState::Paused,
        SessionState::Completed,
        SessionState::Cancelled,
        SessionState::Error,
    ];
    for state in states {
        assert!(
            is_valid_transition(state, SessionState::Compacting),
            "Expected valid transition from {:?} to Compacting",
            state
        );
    }
}

#[test]
fn test_valid_from_compacting() {
    assert!(is_valid_transition(
        SessionState::Compacting,
        SessionState::WaitingForInput
    ));
}

#[test]
fn test_waiting_for_approval_from_any_state() {
    // WaitingForApproval can be entered from any state
    let states = vec![
        SessionState::Initializing,
        SessionState::WaitingForInput,
        SessionState::LlmCalling,
        SessionState::ToolScheduling,
        SessionState::Compacting,
        SessionState::Paused,
        SessionState::Completed,
        SessionState::Cancelled,
        SessionState::Error,
    ];
    for state in states {
        assert!(
            is_valid_transition(state, SessionState::WaitingForApproval),
            "Expected valid transition from {:?} to WaitingForApproval",
            state
        );
    }
}

#[test]
fn test_valid_from_waiting_for_approval() {
    assert!(is_valid_transition(
        SessionState::WaitingForApproval,
        SessionState::ToolScheduling
    ));
    assert!(is_valid_transition(
        SessionState::WaitingForApproval,
        SessionState::Cancelled
    ));
    assert!(is_valid_transition(
        SessionState::WaitingForApproval,
        SessionState::Error
    ));
}

#[test]
fn test_valid_from_paused() {
    assert!(is_valid_transition(
        SessionState::Paused,
        SessionState::WaitingForInput
    ));
}

#[test]
fn test_reset_transitions() {
    assert!(is_valid_transition(
        SessionState::Completed,
        SessionState::Initializing
    ));
    assert!(is_valid_transition(
        SessionState::Cancelled,
        SessionState::Initializing
    ));
    assert!(is_valid_transition(
        SessionState::Error,
        SessionState::Initializing
    ));
}

// --- Invalid transition tests ---

#[test]
fn test_invalid_from_initializing() {
    let invalid_targets = vec![
        SessionState::Initializing,
        SessionState::LlmCalling,
        SessionState::ToolScheduling,
        SessionState::Compacting,
        SessionState::WaitingForApproval,
        SessionState::Paused,
        SessionState::Completed,
        SessionState::Cancelled,
        SessionState::Error,
    ];
    for target in invalid_targets {
        if target == SessionState::Compacting
            || target == SessionState::WaitingForApproval
        {
            continue; // These are valid from any state
        }
        assert!(
            !is_valid_transition(SessionState::Initializing, target),
            "Expected invalid transition from Initializing to {:?}",
            target
        );
    }
}

#[test]
fn test_invalid_completed_to_llm_calling() {
    assert!(!is_valid_transition(
        SessionState::Completed,
        SessionState::LlmCalling
    ));
}

#[test]
fn test_invalid_completed_to_waiting_for_input() {
    assert!(!is_valid_transition(
        SessionState::Completed,
        SessionState::WaitingForInput
    ));
}

#[test]
fn test_invalid_error_to_llm_calling() {
    assert!(!is_valid_transition(
        SessionState::Error,
        SessionState::LlmCalling
    ));
}

#[test]
fn test_invalid_cancelled_to_waiting_for_input() {
    assert!(!is_valid_transition(
        SessionState::Cancelled,
        SessionState::WaitingForInput
    ));
}

#[test]
fn test_invalid_same_state() {
    // Transitions to the same state are generally not valid
    // (except where explicitly allowed, which none are in this design)
    let states = vec![
        SessionState::Initializing,
        SessionState::WaitingForInput,
        SessionState::LlmCalling,
        SessionState::ToolScheduling,
        SessionState::Paused,
        SessionState::Completed,
        SessionState::Cancelled,
        SessionState::Error,
    ];
    for state in states {
        assert!(
            !is_valid_transition(state, state),
            "Expected invalid self-transition for {:?}",
            state
        );
    }
}

// --- Effect tests ---

#[test]
fn test_effect_for_entering_all_states() {
    assert!(matches!(
        effect_for_entering(SessionState::WaitingForApproval),
        StateEnterEffect::StartApprovalTimeout
    ));

    assert!(matches!(
        effect_for_entering(SessionState::ToolScheduling),
        StateEnterEffect::CancelApprovalTimeout
    ));
    assert!(matches!(
        effect_for_entering(SessionState::WaitingForInput),
        StateEnterEffect::CancelApprovalTimeout
    ));

    let normal_states = vec![
        SessionState::Initializing,
        SessionState::LlmCalling,
        SessionState::Compacting,
        SessionState::Paused,
        SessionState::Completed,
        SessionState::Cancelled,
        SessionState::Error,
    ];
    for state in normal_states {
        assert!(
            matches!(effect_for_entering(state), StateEnterEffect::None),
            "Expected no effect for {:?}",
            state
        );
    }
}
