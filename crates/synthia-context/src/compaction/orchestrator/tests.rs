//! Tests for the orchestrator's protection-zone, fallback, and apply-compaction paths.

use synthia_provider::Message;

use super::*;
use crate::compaction::test_providers::{ConstantProvider, FailingProvider};

fn user(s: &str) -> Message {
    Message::user(s)
}

fn assistant(s: &str) -> Message {
    Message::assistant(s)
}

#[test]
fn protection_zone_empty_messages() {
    assert_eq!(calculate_protection_zone(&[], 5, 1000), (0, 0));
}

#[test]
fn protection_zone_no_rounds_returns_zero() {
    let msgs = vec![user("hi")];
    assert_eq!(calculate_protection_zone(&msgs, 0, 1000), (0, 0));
}

#[test]
fn protection_zone_no_user_messages() {
    let msgs = vec![assistant("hello")];
    assert_eq!(calculate_protection_zone(&msgs, 5, 1000), (0, 0));
}

#[test]
fn protection_zone_protects_recent_rounds() {
    let msgs = vec![
        user("q1"),
        assistant("a1"),
        user("q2"),
        assistant("a2"),
        user("q3"),
        assistant("a3"),
    ];
    // min_rounds = 2 → protect from the 2nd-to-last user index.
    let (start, end) = calculate_protection_zone(&msgs, 2, 1_000_000);
    assert_eq!(end, msgs.len());
    // start should be the index of the 2nd-to-last user msg.
    assert!(start <= 4);
}

#[tokio::test]
async fn compact_with_fallback_empty_returns_empty() {
    let provider = ConstantProvider("x".into());
    let result =
        compact_with_fallback(&[], 100, Some(&provider), None, None).await;
    assert!(result.is_empty());
}

#[tokio::test]
async fn compact_with_fallback_l1_succeeds() {
    let provider = ConstantProvider("Summary text".into());
    let msgs = vec![user("hi"), assistant("hello")];
    let result =
        compact_with_fallback(&msgs, 1000, Some(&provider), None, None).await;
    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(text.contains("Summary text"));
}

#[tokio::test]
async fn compact_with_fallback_l1_exceeds_budget_falls_to_l2() {
    // 1-byte budget is impossible for L1 to fit, so it should
    // fall through to L2 structured truncation.
    let provider = ConstantProvider("Summary text".into());
    let msgs = vec![user("hi"), assistant("hello")];
    let result =
        compact_with_fallback(&msgs, 5, Some(&provider), None, None).await;
    assert!(!result.is_empty());
}

#[tokio::test]
async fn compact_with_fallback_no_provider_falls_to_l2() {
    let msgs = vec![user("hi"), assistant("hello")];
    let result = compact_with_fallback(&msgs, 1000, None, None, None).await;
    // L2 path: result mirrors the input messages.
    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn compact_with_fallback_l2_exceeds_falls_to_l3() {
    // No provider → L1 path is skipped; L2 keeps the full user
    // message ("hi") which exceeds the 5-token budget; L3 wins.
    let msgs = vec![user("hi"), assistant("hello")];
    let result = compact_with_fallback(&msgs, 5, None, None, None).await;
    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(
        text.contains("call-completed") || text.contains("compacted"),
        "expected L3 marker or placeholder, got: {text}"
    );
}

#[tokio::test]
async fn apply_compaction_empty_messages() {
    let provider = ConstantProvider("x".into());
    let result = apply_compaction(&[], 0..0, 1000, Some(&provider), None)
        .await
        .unwrap();
    assert_eq!(result.applied_level, 0);
    assert_eq!(result.compacted_indices.len(), 0);
}

#[tokio::test]
async fn apply_compaction_empty_range() {
    let provider = ConstantProvider("x".into());
    let msgs = vec![user("hi")];
    let result = apply_compaction(&msgs, 0..0, 1000, Some(&provider), None)
        .await
        .unwrap();
    assert_eq!(result.applied_level, 0);
}

#[tokio::test]
async fn apply_compaction_l1_success() {
    let provider = ConstantProvider("S".into());
    let msgs = vec![user("hi"), assistant("hello")];
    let result = apply_compaction(&msgs, 0..2, 1000, Some(&provider), None)
        .await
        .unwrap();
    assert_eq!(result.applied_level, 1);
    assert!(result.summary.summary.contains("S"));
}

#[tokio::test]
async fn apply_compaction_no_provider_falls_to_l2() {
    let msgs = vec![user("hi"), assistant("hello")];
    let result = apply_compaction(&msgs, 0..2, 1000, None, None)
        .await
        .unwrap();
    assert_eq!(result.applied_level, 2);
}

#[tokio::test]
async fn apply_compaction_single_pass_original_tokens_at_l3() {
    // L1 fails (FailingProvider), L2 fails (budget = 1), L3 wins.
    // original_tokens must be reported once, not recomputed.
    let provider = FailingProvider;
    let msgs = vec![user("hi"), assistant("hello")];
    let result = apply_compaction(&msgs, 0..2, 1, Some(&provider), None)
        .await
        .unwrap();
    assert_eq!(result.applied_level, 3);
    assert!(result.original_tokens > 0);
}

#[tokio::test]
async fn compact_with_fallback_threads_previous_summary() {
    use parking_lot::Mutex;

    use crate::compaction::test_providers::CapturingProvider;
    let provider = CapturingProvider {
        last_previous: Mutex::new(None),
        summary: "Truncated-anchored summary".into(),
    };
    let msgs = vec![user("hi"), assistant("hello")];
    let _ = compact_with_fallback(
        &msgs,
        10_000,
        Some(&provider),
        Some("anchor"),
        None,
    )
    .await;
    assert_eq!(provider.last_previous.lock().as_deref(), Some("anchor"));
}

#[tokio::test]
async fn compact_with_fallback_forwards_precomputed_to_l1() {
    let provider = ConstantProvider("S".into());
    let msgs = vec![user("hi")];
    let result =
        compact_with_fallback(&msgs, 10_000, Some(&provider), None, Some(9999))
            .await;
    // L1 returned 1 message; we don't reach into its internals
    // here — that's covered by level1 tests. This test just
    // makes sure the call doesn't panic when the precomputed
    // value is supplied through compact_with_fallback.
    assert!(!result.is_empty());
}
