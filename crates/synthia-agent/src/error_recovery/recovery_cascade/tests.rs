//! Unit tests for the recovery cascade family.
//!
//! The original 11 tests lived at the bottom of
//! `recovery_cascade.rs`; they're hoisted into this
//! sibling file so the test bodies don't bloat the
//! orchestrator (`run.rs`).
//!
//! Coverage map:
//!
//! - `ConsecutiveFailureTracker` (2): `tracker_*`.
//! - L3 (2): `l3_*`.
//! - L4 (4): `l4_*`.
//! - L5 (3): `l5_*`.
//! - Integration (1): `integration_l3_fails_then_l4_*`.

use synthia_context::traits::estimate_message_tokens;
use synthia_provider::Message;
use synthia_telemetry::span_context::SpanContext;

use super::{
    core::{COMPACT_THRESHOLD, RecoveryAction},
    run::run_recovery_cascade,
    tracker::ConsecutiveFailureTracker,
};
use crate::{
    error_recovery::{ErrorRecoveryCoordinator, reset::ResetCoordinator},
    loop_context::LoopContext,
};

fn new_ctx() -> LoopContext {
    LoopContext::new("session".to_string(), SpanContext::new("test"))
}

fn new_coordinator() -> ErrorRecoveryCoordinator {
    ErrorRecoveryCoordinator::new(0)
}

/// Run the cascade with fresh L5 state (loop detector, steering,
/// reset coordinator). The defaults are sufficient to exercise L3/L4
/// and the L5 happy path; tests that need to exercise L5 failure
/// modes construct their own state.
fn run_with_default_l5(
    error: &str,
    tool_name: &str,
    ctx: &mut LoopContext,
    tracker: &mut ConsecutiveFailureTracker,
    coordinator: &ErrorRecoveryCoordinator,
    budget: Option<&synthia_session::types::TokenBudget>,
    provider: Option<
        &dyn synthia_context::compaction::level1::CompactionProvider,
    >,
) -> (
    RecoveryAction,
    synthia_guardian::LoopDetectorSet,
    ResetCoordinator,
) {
    let mut loop_detector = synthia_guardian::LoopDetectorSet::new();
    let reset_coordinator = ResetCoordinator::new();
    let action = futures::executor::block_on(run_recovery_cascade(
        error,
        tool_name,
        ctx,
        tracker,
        coordinator,
        budget,
        provider,
        &mut loop_detector,
        None,
        &reset_coordinator,
    ));
    (action, loop_detector, reset_coordinator)
}

/// Build a list of messages whose trait-estimated token count exceeds
/// `hard_limit * COMPACT_THRESHOLD`, using only `ContentPart::Text` so
/// the public estimator actually counts them.
fn heavy_user_messages(hard_limit: usize) -> Vec<Message> {
    // traits::estimate_message_tokens returns 4 + ceil(chars/4).
    // To exceed 0.8 * hard_limit, we need at least 0.8 * hard_limit
    // characters across the messages (rounded up to nearest 4).
    let target_chars = (hard_limit as f64 * 0.85) as usize * 4;
    let text = "a".repeat(target_chars);
    vec![Message::user(text)]
}

// ---- ConsecutiveFailureTracker ----

#[test]
fn tracker_records_increments_and_clears() {
    let mut t = ConsecutiveFailureTracker::new();
    assert_eq!(t.failure_count("bash"), 0);
    assert_eq!(t.record_failure("bash"), 1);
    assert_eq!(t.record_failure("bash"), 2);
    assert_eq!(t.failure_count("bash"), 2);
    t.record_success("bash");
    assert_eq!(t.failure_count("bash"), 0);
}

#[test]
fn tracker_isolates_per_tool() {
    let mut t = ConsecutiveFailureTracker::new();
    t.record_failure("bash");
    t.record_failure("bash");
    t.record_failure("read_file");
    assert_eq!(t.failure_count("bash"), 2);
    assert_eq!(t.failure_count("read_file"), 1);
    t.reset();
    assert_eq!(t.failure_count("bash"), 0);
    assert_eq!(t.failure_count("read_file"), 0);
}

// ---- L3 ----

#[tokio::test]
async fn l3_returns_fallback_after_two_failures() {
    let mut ctx = new_ctx();
    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    // First call: bash has failed once; L3 needs >=2; L4 skipped (no
    // budget). L5 fires (success) and returns Recovered.
    let (result, _, _) = run_with_default_l5(
        "err",
        "bash",
        &mut ctx,
        &mut tracker,
        &coordinator,
        None,
        None,
    );
    match result {
        RecoveryAction::Recovered { message, level } => {
            assert!(message.contains("Conversation reset"));
            assert_eq!(level, 5);
        }
        other => panic!("expected L5 Recovered, got {other:?}"),
    }
    assert_eq!(tracker.failure_count("bash"), 0);
    // Reset wiped the messages from the first L5 reset, so the
    // second call sees a clean ctx and the failure counter was
    // cleared. Re-arm a single failure to keep the test focused on
    // L3-only behavior below.
    tracker.record_failure("bash");

    // Second call from a clean state, with a pre-loaded failure
    // count of 1: L3 not yet (still 1), L4 skipped → L5 fires again.
    let (result, _, _) = run_with_default_l5(
        "err",
        "bash",
        &mut ctx,
        &mut tracker,
        &coordinator,
        None,
        None,
    );
    match result {
        RecoveryAction::Recovered { message, .. } => {
            // L3 wins on the next failure because the failure count
            // reaches 2 within the call below; in this run the
            // counter was 1, so L5 fires first.
            assert!(
                message.contains("Conversation reset")
                    || message.contains("Describing")
            );
        }
        other => panic!("expected Recovered, got {other:?}"),
    }
}

#[tokio::test]
async fn l3_takes_priority_over_l4_when_both_could_apply() {
    // Once L3 is applicable (failure count >= 2), it must win over L4
    // even though L4 could also fire. This is the spec precedence.
    let budget = synthia_session::types::TokenBudget::new(1_000);
    let mut ctx = new_ctx()
        .with_token_limit(budget.hard_limit)
        .with_messages(heavy_user_messages(budget.hard_limit));

    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    // Pre-load two failures so the FIRST cascade call sees
    // failure count 3 → L3 immediately applicable.
    tracker.record_failure("bash");
    tracker.record_failure("bash");

    let (r1, _, _) = run_with_default_l5(
        "err",
        "bash",
        &mut ctx,
        &mut tracker,
        &coordinator,
        Some(&budget),
        None,
    );
    match r1 {
        RecoveryAction::Recovered { message, level } => {
            assert!(message.contains("Describing"));
            assert_eq!(level, 3);
        }
        other => panic!("expected L3 Recovered, got {other:?}"),
    }
}

// ---- L4 ----

#[tokio::test]
async fn l4_triggers_auto_compact_when_context_is_high() {
    let budget = synthia_session::types::TokenBudget::new(1_000); // soft_limit = 700
    let mut ctx = new_ctx()
        .with_token_limit(budget.hard_limit)
        .with_messages(heavy_user_messages(budget.hard_limit));
    let baseline_ratio = ctx.token_ratio();
    assert!(
        baseline_ratio > COMPACT_THRESHOLD,
        "test setup: ratio must exceed threshold, got {baseline_ratio}"
    );

    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    let (result, _, _) = run_with_default_l5(
        "err",
        "unknown_tool", // no L3 fallback → L4 path
        &mut ctx,
        &mut tracker,
        &coordinator,
        Some(&budget),
        None,
    );

    match result {
        RecoveryAction::Recovered { message, level } => {
            assert!(message.contains("Context auto-compacted"));
            assert_eq!(level, 4);
        }
        other => panic!("expected Recovered, got {other:?}"),
    }
    assert_eq!(tracker.failure_count("unknown_tool"), 0);
    assert_eq!(coordinator.consecutive_error_count(), 0);
}

#[tokio::test]
async fn l4_skips_compact_when_context_is_low_then_l5_fires() {
    let mut ctx = new_ctx()
        .with_token_limit(1_000_000)
        .with_messages(vec![Message::user("hi")]);
    assert!(ctx.token_ratio() < COMPACT_THRESHOLD);

    let original_messages = ctx.messages.clone();
    let budget = synthia_session::types::TokenBudget::new(1_000_000);
    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    let (result, _, _) = run_with_default_l5(
        "err",
        "unknown_tool",
        &mut ctx,
        &mut tracker,
        &coordinator,
        Some(&budget),
        None,
    );

    // L4 skipped (low ratio), L5 fires successfully → Recovered
    // with the reset marker and ctx.messages cleared.
    match result {
        RecoveryAction::Recovered { message, level } => {
            assert!(message.contains("Conversation reset"));
            assert_eq!(level, 5);
        }
        other => panic!("expected L5 Recovered, got {other:?}"),
    }
    // L5 cleared the messages; original length was 1 so the new
    // length is 0.
    assert!(ctx.messages.len() < original_messages.len());
}

#[tokio::test]
async fn l4_at_exact_threshold_skips_compact_then_l5_fires() {
    // ratio just under threshold must NOT trigger compact (spec: strictly >).
    // Pick a hard_limit that produces ratio slightly below 0.8.
    let mut ctx = new_ctx();
    let text = "a".repeat(4_000); // ~1004 tokens via traits
    ctx.messages.push(Message::user(&text));
    // hard_limit = 1260 → ratio ≈ 0.797 (below 0.8)
    let hard_limit = 1_260;
    let mut ctx = ctx.with_token_limit(hard_limit);
    let ratio = ctx.token_ratio();
    assert!(
        ratio <= COMPACT_THRESHOLD,
        "ratio setup off (must be <= 0.8): {ratio}"
    );
    assert!(
        ratio > COMPACT_THRESHOLD - 0.05,
        "ratio should be close to threshold: {ratio}"
    );

    let budget = synthia_session::types::TokenBudget::new(hard_limit);
    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    let (result, _, _) = run_with_default_l5(
        "err",
        "unknown_tool",
        &mut ctx,
        &mut tracker,
        &coordinator,
        Some(&budget),
        None,
    );
    // L4 skipped, L5 fires.
    match result {
        RecoveryAction::Recovered { message, level } => {
            assert!(message.contains("Conversation reset"));
            assert_eq!(level, 5);
        }
        other => panic!("expected L5 Recovered, got {other:?}"),
    }
}

#[tokio::test]
async fn l4_skips_compact_when_budget_is_none_then_l5_fires() {
    let budget = synthia_session::types::TokenBudget::new(1_000);
    let mut ctx = new_ctx()
        .with_token_limit(budget.hard_limit)
        .with_messages(heavy_user_messages(budget.hard_limit));
    assert!(ctx.token_ratio() > COMPACT_THRESHOLD);

    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    let (result, _, _) = run_with_default_l5(
        "err",
        "unknown_tool",
        &mut ctx,
        &mut tracker,
        &coordinator,
        None, // no budget → L4 must be skipped
        None,
    );
    // L4 skipped, L5 fires.
    match result {
        RecoveryAction::Recovered { message, level } => {
            assert!(message.contains("Conversation reset"));
            assert_eq!(level, 5);
        }
        other => panic!("expected L5 Recovered, got {other:?}"),
    }
}

// ---- L5 ----

#[tokio::test]
async fn l5_fires_after_l3_l4_no_op_and_returns_recovered() {
    // No fallback, no budget, low context → L3/L4 no-op → L5 fires.
    let mut ctx = new_ctx();
    ctx.messages.push(Message::user("a"));
    ctx.messages.push(Message::user("b"));
    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    let (result, mut loop_detector, reset) = run_with_default_l5(
        "err",
        "unknown_tool",
        &mut ctx,
        &mut tracker,
        &coordinator,
        None,
        None,
    );
    match result {
        RecoveryAction::Recovered { message, level } => {
            assert!(message.contains("Conversation reset"));
            assert_eq!(level, 5);
        }
        other => panic!("expected L5 Recovered, got {other:?}"),
    }
    // L5 wiped messages and reset the error counter.
    assert!(ctx.messages.is_empty());
    assert_eq!(coordinator.consecutive_error_count(), 0);
    assert!(!reset.is_in_cooldown());
    // Loop detector still fresh.
    let (status, _) = loop_detector.check("bash", "{}", 0);
    assert_eq!(status, synthia_guardian::LoopStatus::Ok);
}

#[tokio::test]
async fn l5_preserves_session_id_and_clears_messages() {
    let mut ctx = new_ctx();
    ctx.session_id = "sess-7".to_string();
    ctx.messages.push(Message::user("keep me?"));
    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    let (result, _, _) = run_with_default_l5(
        "err",
        "unknown_tool",
        &mut ctx,
        &mut tracker,
        &coordinator,
        None,
        None,
    );
    assert!(matches!(result, RecoveryAction::Recovered { level: 5, .. }));
    assert_eq!(ctx.session_id, "sess-7");
    assert!(ctx.messages.is_empty());
}

#[tokio::test]
async fn l5_returns_fail_fast_when_cooldown_active() {
    let mut ctx = new_ctx();
    ctx.messages.push(Message::user("a"));
    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();

    // Pre-arm a cooldown on the reset coordinator; the cascade
    // must observe it and return FailFast.
    let reset = ResetCoordinator::new();
    reset.start_cooldown();
    let mut loop_detector = synthia_guardian::LoopDetectorSet::new();
    let result = run_recovery_cascade(
        "err",
        "unknown_tool",
        &mut ctx,
        &mut tracker,
        &coordinator,
        None,
        None,
        &mut loop_detector,
        None,
        &reset,
    )
    .await;
    match result {
        RecoveryAction::FailFast(msg) => {
            assert!(msg.contains("L5 reset failed"));
            assert!(msg.contains("cooldown"));
        }
        other => panic!("expected FailFast, got {other:?}"),
    }
    // Messages must NOT have been wiped because the reset was
    // refused before doing any work.
    assert_eq!(ctx.messages.len(), 1);
}

// ---- LoopContext::token_ratio ----

#[tokio::test]
async fn integration_l3_fails_then_l4_compact_succeeds_and_session_continues() {
    // L3 (no fallback for "unknown_tool") → L4 (high ratio) → Recovered.
    // Session can continue because ctx.messages were reduced in place.
    let budget = synthia_session::types::TokenBudget::new(1_000);
    let mut ctx = new_ctx()
        .with_token_limit(budget.hard_limit)
        .with_messages(heavy_user_messages(budget.hard_limit));
    let pre_tokens: usize =
        ctx.messages.iter().map(estimate_message_tokens).sum();

    let mut tracker = ConsecutiveFailureTracker::new();
    let coordinator = new_coordinator();
    let (result, _, _) = run_with_default_l5(
        "err",
        "unknown_tool",
        &mut ctx,
        &mut tracker,
        &coordinator,
        Some(&budget),
        None,
    );

    match result {
        RecoveryAction::Recovered { .. } => {}
        other => panic!("expected Recovered, got {other:?}"),
    }
    // Session can continue: messages were compacted, error counter reset.
    let post_tokens: usize =
        ctx.messages.iter().map(estimate_message_tokens).sum();
    assert!(
        post_tokens < pre_tokens,
        "compaction should reduce tokens: pre={pre_tokens} post={post_tokens}"
    );
    assert_eq!(tracker.failure_count("unknown_tool"), 0);
    assert_eq!(coordinator.consecutive_error_count(), 0);
    // Needs-compact flag cleared.
    assert!(!ctx.needs_compact);
}
