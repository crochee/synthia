//! Unit tests for the `reset` module family.
//!
//! Coverage map (16 tests):
//!
//! - `determine_scope`: 3 tests (few / medium / many errors).
//! - `is_safe_to_reset`: 2 tests (no / with unsaved work).
//! - `ResetResult` builders: 2 tests (`success` / `failed`).
//! - `DeadlockPrevention`: 4 tests (`is_deadlocked` / boundary /
//!   `default_threshold_secs` / `reset_timeout_secs`).
//! - Conversation reset behavior: 3 tests (discards messages /
//!   clears loop detector / drains steering channel).
//! - L5 fallback behavior: 2 tests (ToolState fallback / Full fallback).
//! - Cooldown behavior: 2 tests (`execute_refuses_during_cooldown` /
//!   `clear_cooldown_disables_cooldown`).

use synthia_guardian::LoopDetectorSet;
use synthia_provider::Message;
use synthia_telemetry::span_context::SpanContext;

use super::*;
use crate::{
    error_recovery::ErrorRecoveryCoordinator,
    loop_context::LoopContext,
    steering::SteeringChannel,
};

fn new_ctx_with_session(session_id: &str) -> LoopContext {
    LoopContext::new(session_id.to_string(), SpanContext::new(session_id))
}

#[test]
fn test_reset_scope_few_errors() {
    assert_eq!(
        ResetCoordinator::determine_scope(0),
        ResetScope::Conversation
    );
    assert_eq!(
        ResetCoordinator::determine_scope(5),
        ResetScope::Conversation
    );
}

#[test]
fn test_reset_scope_medium_errors() {
    assert_eq!(ResetCoordinator::determine_scope(6), ResetScope::ToolState);
    assert_eq!(ResetCoordinator::determine_scope(10), ResetScope::ToolState);
}

#[test]
fn test_reset_scope_many_errors() {
    assert_eq!(ResetCoordinator::determine_scope(11), ResetScope::Full);
    assert_eq!(ResetCoordinator::determine_scope(100), ResetScope::Full);
}

#[test]
fn test_is_safe_to_reset_no_unsaved_work() {
    assert!(ResetCoordinator::is_safe_to_reset(false));
}

#[test]
fn test_is_safe_to_reset_with_unsaved_work() {
    assert!(!ResetCoordinator::is_safe_to_reset(true));
}

#[test]
fn test_reset_result_success() {
    let result = ResetCoordinator::conversation_reset();
    assert!(result.success);
    assert_eq!(result.scope, ResetScope::Conversation);
}

#[test]
fn test_reset_result_failed() {
    let result = ResetCoordinator::reset_failed("test reason");
    assert!(!result.success);
    assert_eq!(result.scope, ResetScope::Full);
    assert!(result.description.contains("test reason"));
}

#[test]
fn test_deadlock_detection() {
    assert!(DeadlockPrevention::is_deadlocked(1000, 1200, 100));
    assert!(!DeadlockPrevention::is_deadlocked(1000, 1050, 100));
}

#[test]
fn test_deadlock_threshold_boundary() {
    assert!(!DeadlockPrevention::is_deadlocked(1000, 1100, 100));
    assert!(DeadlockPrevention::is_deadlocked(1000, 1101, 100));
}

#[test]
fn test_deadlock_default_threshold() {
    assert_eq!(DeadlockPrevention::default_threshold_secs(), 120);
}

#[test]
fn test_reset_timeout() {
    assert_eq!(DeadlockPrevention::reset_timeout_secs(), 30);
}

// ---- 5.8: reset discards context, preserves session metadata ----

#[test]
fn execute_conversation_discards_messages_and_preserves_session() {
    let coordinator = ResetCoordinator::new();
    let recovery = ErrorRecoveryCoordinator::new(0);
    // Pre-load the recovery coordinator with errors so we can prove
    // they get reset.
    recovery
        .handle_error("err1", crate::error_recovery::RecoveryLevel::L1Truncate);
    recovery
        .handle_error("err2", crate::error_recovery::RecoveryLevel::L1Truncate);
    assert!(recovery.consecutive_error_count() > 0);

    let mut ctx = new_ctx_with_session("session-abc");
    ctx.messages.push(Message::user("first"));
    ctx.messages.push(Message::user("second"));
    ctx.cumulative_tokens = 9_999;
    ctx.recent_tool_results
        .push(("bash".to_string(), "ok".to_string(), true));
    ctx.iteration = 7;
    ctx.needs_compact = true;

    let mut loop_detector = LoopDetectorSet::new();
    let _ = loop_detector.check("bash", "{}", 0);
    let _ = loop_detector.check("bash", "{}", 1);

    let result = coordinator.execute(
        ResetScope::Conversation,
        &mut ctx,
        &mut loop_detector,
        None,
        &recovery,
    );

    assert!(
        result.success,
        "reset should succeed: {}",
        result.description
    );
    assert_eq!(result.scope, ResetScope::Conversation);

    // Messages / tool history / counters cleared.
    assert!(ctx.messages.is_empty(), "messages should be discarded");
    assert!(ctx.recent_tool_results.is_empty());
    assert_eq!(ctx.cumulative_tokens, 0);
    assert_eq!(ctx.iteration, 0);
    assert!(!ctx.needs_compact);

    // Session metadata preserved (SpanContext is not PartialEq, so
    // we compare its inner session_id).
    assert_eq!(ctx.session_id, "session-abc");
    assert_eq!(ctx.span_ctx.session_id(), "session-abc");

    // Consecutive error counter reset.
    assert_eq!(recovery.consecutive_error_count(), 0);
}

// ---- 5.9: loop detector state is cleared after reset ----

#[test]
fn execute_conversation_clears_loop_detector_state() {
    let coordinator = ResetCoordinator::new();
    let recovery = ErrorRecoveryCoordinator::new(0);
    let mut ctx = new_ctx_with_session("s");
    let mut loop_detector = LoopDetectorSet::new();

    // Build state across multiple detectors.
    // Doom loop window: 2 identical calls.
    let _ = loop_detector.check("bash", "{}", 0);
    let _ = loop_detector.check("bash", "{}", 1);
    // GenericRepeat counter.
    let _ = loop_detector.check("read_file", "args1", 2);
    let _ = loop_detector.check("read_file", "args1", 3);
    // Poll no-progress.
    let _ = loop_detector.check_poll_result("same");

    // After reset, all detectors should be back to fresh state.
    let result = coordinator.execute(
        ResetScope::Conversation,
        &mut ctx,
        &mut loop_detector,
        None,
        &recovery,
    );
    assert!(result.success);

    // First call after reset: DoomLoop window must be empty, so
    // identical call cannot fire on a single observation.
    let (status, action) = loop_detector.check("bash", "{}", 0);
    assert_eq!(status, synthia_guardian::LoopStatus::Ok);
    assert_eq!(action, None);
    // GenericRepeat for the same (tool, args) must start at 0.
    let (status, _) = loop_detector.check("read_file", "args1", 1);
    assert_eq!(status, synthia_guardian::LoopStatus::Ok);
}

#[test]
fn execute_conversation_drains_steering_channel() {
    use crate::steering::{MpscSteeringChannel, SteeringMessage};

    let coordinator = ResetCoordinator::new();
    let recovery = ErrorRecoveryCoordinator::new(0);
    let mut ctx = new_ctx_with_session("s");
    let mut loop_detector = LoopDetectorSet::new();
    let steering = MpscSteeringChannel::new();

    // Send a few steering messages; executor must drain them.
    futures::executor::block_on(async {
        steering.send(SteeringMessage::new("a")).await;
        steering.send(SteeringMessage::new("b")).await;
    });
    assert!(!steering.is_empty());

    let result = coordinator.execute(
        ResetScope::Conversation,
        &mut ctx,
        &mut loop_detector,
        Some(&steering),
        &recovery,
    );
    assert!(result.success);
    assert!(steering.is_empty(), "steering channel must be drained");
}

// ---- L5 fallback: ToolState / Full fall back to Conversation ----

#[test]
fn tool_state_range_resets_via_conversation_fallback() {
    // consecutive_errors=7 → determine_scope → ToolState
    let scope = ResetCoordinator::determine_scope(7);
    assert_eq!(scope, ResetScope::ToolState);

    let coordinator = ResetCoordinator::new();
    let recovery = ErrorRecoveryCoordinator::new(0);
    let mut ctx = new_ctx_with_session("s");
    ctx.messages.push(Message::user("data"));
    let mut loop_detector = LoopDetectorSet::new();

    // No cooldown initially.
    assert!(!coordinator.is_in_cooldown());

    // ToolState falls back to Conversation reset and succeeds.
    let result = coordinator.execute(
        scope,
        &mut ctx,
        &mut loop_detector,
        None,
        &recovery,
    );
    assert!(
        result.success,
        "ToolState should fall back to Conversation: {}",
        result.description
    );
    assert_eq!(result.scope, ResetScope::Conversation);
    // Cooldown must NOT be active after a successful reset.
    assert!(
        !coordinator.is_in_cooldown(),
        "cooldown must not start after successful fallback"
    );
    // Messages were cleared by the Conversation reset.
    assert!(ctx.messages.is_empty());
}

#[test]
fn full_scope_resets_via_conversation_fallback() {
    // consecutive_errors=12 → determine_scope → Full
    let scope = ResetCoordinator::determine_scope(12);
    assert_eq!(scope, ResetScope::Full);

    let coordinator = ResetCoordinator::new();
    let recovery = ErrorRecoveryCoordinator::new(0);
    let mut ctx = new_ctx_with_session("s");
    ctx.messages.push(Message::user("data"));
    let mut loop_detector = LoopDetectorSet::new();

    assert!(!coordinator.is_in_cooldown());

    // Full falls back to Conversation reset and succeeds.
    let result = coordinator.execute(
        scope,
        &mut ctx,
        &mut loop_detector,
        None,
        &recovery,
    );
    assert!(
        result.success,
        "Full should fall back to Conversation: {}",
        result.description
    );
    assert_eq!(result.scope, ResetScope::Conversation);
    assert!(
        !coordinator.is_in_cooldown(),
        "cooldown must not start after successful fallback"
    );
    assert!(ctx.messages.is_empty());
}

#[test]
fn execute_refuses_during_cooldown() {
    let coordinator = ResetCoordinator::new();
    let recovery = ErrorRecoveryCoordinator::new(0);
    let mut ctx = new_ctx_with_session("s2");
    ctx.messages.push(Message::user("kept"));
    let mut loop_detector = LoopDetectorSet::new();

    coordinator.start_cooldown();
    assert!(coordinator.is_in_cooldown());

    let result = coordinator.execute(
        ResetScope::Conversation,
        &mut ctx,
        &mut loop_detector,
        None,
        &recovery,
    );
    assert!(!result.success);
    assert!(result.description.contains("cooldown"));
    // Messages must NOT have been touched during cooldown.
    assert_eq!(
        ctx.messages.len(),
        1,
        "messages must be untouched in cooldown"
    );
}

#[test]
fn clear_cooldown_disables_cooldown() {
    let coordinator = ResetCoordinator::new();
    coordinator.start_cooldown();
    assert!(coordinator.is_in_cooldown());
    coordinator.clear_cooldown();
    assert!(!coordinator.is_in_cooldown());
}
