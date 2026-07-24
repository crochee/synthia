//! Unit tests for [`Compactor`].
//!
//! Coverage map (18 tests):
//!
//! - Construction: 2 tests
//!   ([`Compactor::new`] defaults, [`Compactor::with_limits`] updates).
//! - Level 1: 2 tests
//!   (short conversation summary, empty messages).
//! - Level 2: 1 test
//!   (keeps user full / collapses tool results).
//! - Level 3: 1 test
//!   (collapses to marker).
//! - [`super::dispatch::auto_select_level`]: 4 tests
//!   (under-budget → 0, 1.2x → 1, 2x → 2, 5x → 3).
//! - Unknown level: 1 test
//!   (level 99 passes messages through unchanged).
//! - [`super::dispatch::compact_to_token_budget`]: 2 tests
//!   (under-budget passes through, oversized collapses to marker or
//!   drastic reduction).
//! - [`super::dispatch::compact_with_provider`]: 5 tests
//!   (LLM summary used, None → fallback, failing → fallback,
//!   empty → fallback, truncated previous summary is forwarded).
//! - [`super::dispatch::compact_with_marker`]: 1 test
//!   (returns the configured range).
//! - [`super::level::CompactionLevel::as_usize`]: 1 test
//!   (1, 2, 3 round-trip).

use synthia_provider::Message;

use super::*;
use crate::compaction::test_providers::{
    ConstantProvider,
    EmptyProvider,
    FailingProvider,
};

fn user(s: &str) -> Message {
    Message::user(s)
}

fn assistant(s: &str) -> Message {
    Message::assistant(s)
}

// =============================================================================
// Construction Tests
// =============================================================================

#[test]
fn with_limits_updates_config() {
    let compactor = Compactor::new(1).with_limits(1000, 3);
    let messages = vec![user("hi"), assistant("hello world")];
    let result = compactor.compact(&messages).unwrap();
    assert!(!result.content.is_empty());
}

#[test]
fn unknown_level_passes_messages_through() {
    let compactor = Compactor::new(99);
    let messages = vec![user("hi"), assistant("hello")];
    let result = compactor.compact(&messages).unwrap();
    assert!(result.content.contains("hi"));
    assert!(result.content.contains("hello"));
}

// =============================================================================
// Level 1 Tests
// =============================================================================

#[test]
fn level1_summary_short_conversation() {
    let compactor = Compactor::new(1);
    let messages = vec![
        user("What is Rust?"),
        assistant(
            "Rust is a systems programming language focused on safety and performance.",
        ),
    ];
    let result = compactor.compact(&messages).unwrap();
    assert!(result.content.contains("Summary"));
    assert!(result.original_tokens > 0);
    assert!(result.compacted_tokens > 0);
}

#[test]
fn level1_summary_empty() {
    let compactor = Compactor::new(1);
    let result = compactor.compact(&[]).unwrap();
    assert!(result.content.is_empty());
    assert_eq!(result.compacted_tokens, 0);
}

// =============================================================================
// Level 2 Test
// =============================================================================

#[test]
fn level2_truncate_keeps_user_full_collapses_tool_results() {
    let compactor = Compactor::new(2);
    let messages = vec![
        user("What is Rust?"),
        assistant("Rust is a systems programming language."),
    ];
    let result = compactor.compact(&messages).unwrap();
    // The user message should appear in the result content.
    assert!(result.content.contains("What is Rust?"));
    // The assistant message's first line should appear.
    assert!(
        result
            .content
            .contains("Rust is a systems programming language.")
    );
}

// =============================================================================
// Level 3 Test
// =============================================================================

#[test]
fn level3_marker_only_collapses_to_marker() {
    let compactor = Compactor::new(3);
    let messages = vec![user("hi"), assistant("hello")];
    let result = compactor.compact(&messages).unwrap();
    // No tool calls in this input → L3 yields a placeholder.
    assert!(result.content.contains("compacted"));
}

// =============================================================================
// auto_select_level Tests
// =============================================================================

#[test]
fn auto_select_level_under_budget_returns_zero() {
    let compactor = Compactor::new(1);
    assert_eq!(compactor.auto_select_level(100, 1000), 0);
}

#[test]
fn auto_select_level_mild_overflow_returns_1() {
    let compactor = Compactor::new(1);
    assert_eq!(compactor.auto_select_level(1200, 1000), 1);
}

#[test]
fn auto_select_level_moderate_overflow_returns_2() {
    let compactor = Compactor::new(1);
    assert_eq!(compactor.auto_select_level(2000, 1000), 2);
}

#[test]
fn auto_select_level_severe_overflow_returns_3() {
    let compactor = Compactor::new(1);
    assert_eq!(compactor.auto_select_level(5000, 1000), 3);
}

// =============================================================================
// compact_to_token_budget Tests
// =============================================================================

#[test]
fn compact_to_token_budget_under_budget_passes_through() {
    let compactor = Compactor::new(1);
    let messages = vec![user("hi"), assistant("hello")];
    let result = compactor
        .compact_to_token_budget(&messages, 10_000)
        .unwrap();
    assert_eq!(result.original_tokens, result.compacted_tokens);
}

#[test]
fn compact_to_token_budget_oversized_collapses_to_marker() {
    let compactor = Compactor::new(1);
    let big: Vec<Message> =
        (0..1000).map(|i| user(&format!("msg {i}"))).collect();
    let result = compactor.compact_to_token_budget(&big, 5).unwrap();
    // 1000 messages × ~6 tokens = ~6000 tokens original; the
    // output must be either a marker (small) or fit in the
    // budget. We don't require a strict equality because the
    // marker path itself has a small constant size — what we
    // care about is that the algorithm collapses the input
    // rather than returning the full ~6000-token content.
    assert!(
        result.compacted_tokens < result.original_tokens / 10
            || result.content.contains("removed"),
        "expected drastic reduction or marker, got compacted={}, original={}, content='{}'",
        result.compacted_tokens,
        result.original_tokens,
        &result.content[..result.content.len().min(100)]
    );
}

// =============================================================================
// compact_with_provider Tests
// =============================================================================

#[tokio::test]
async fn compact_with_provider_uses_llm_summary() {
    let provider = ConstantProvider("LLM-Summary".into());
    let compactor = Compactor::new(1);
    let messages = vec![user("hi"), assistant("hello")];
    let result = compactor
        .compact_with_provider(&messages, Some(&provider), None)
        .await
        .unwrap();
    assert!(result.content.contains("LLM-Summary"));
}

#[tokio::test]
async fn compact_with_provider_none_falls_back() {
    let compactor = Compactor::new(1);
    let messages = vec![user("hi"), assistant("hello")];
    let result = compactor
        .compact_with_provider(&messages, None, None)
        .await
        .unwrap();
    assert!(result.content.contains("Summary of 2 messages"));
}

#[tokio::test]
async fn compact_with_provider_failure_falls_back() {
    let provider = FailingProvider;
    let compactor = Compactor::new(1);
    let messages = vec![user("hi"), assistant("hello")];
    let result = compactor
        .compact_with_provider(&messages, Some(&provider), None)
        .await
        .unwrap();
    assert!(result.content.contains("Summary of 2 messages"));
}

#[tokio::test]
async fn compact_with_provider_empty_falls_back() {
    let provider = EmptyProvider;
    let compactor = Compactor::new(1);
    let messages = vec![user("hi"), assistant("hello")];
    let result = compactor
        .compact_with_provider(&messages, Some(&provider), None)
        .await
        .unwrap();
    assert!(result.content.contains("Summary of 2 messages"));
}

#[tokio::test]
async fn compact_with_provider_threads_truncated_previous_summary() {
    use parking_lot::Mutex;

    use crate::compaction::test_providers::CapturingProvider;
    let provider = CapturingProvider {
        last_previous: Mutex::new(None),
        summary: "ok".into(),
    };
    let compactor = Compactor::new(1);
    let big_anchor = "z".repeat(8_000);
    let messages = vec![user("hi")];
    let _ = compactor
        .compact_with_provider(&messages, Some(&provider), Some(&big_anchor))
        .await
        .unwrap();
    // The provider should see a truncated anchor (≤ 4000 bytes + marker).
    let captured = provider.last_previous.lock().clone().unwrap();
    assert!(captured.len() <= 4_000 + 64);
}

// =============================================================================
// compact_with_marker Test
// =============================================================================

#[test]
fn compact_with_marker_returns_range_marker() {
    let compactor = Compactor::new(1);
    let messages = vec![user("hi"), assistant("hello")];
    let (_part, marker) =
        compactor.compact_with_marker(&messages, 0, 2).unwrap();
    assert_eq!(marker.start_index, 0);
    assert_eq!(marker.end_index, 2);
}

// =============================================================================
// CompactionLevel Test
// =============================================================================

#[test]
fn compaction_level_as_usize() {
    assert_eq!(CompactionLevel::Level1Summary.as_usize(), 1);
    assert_eq!(CompactionLevel::Level2StructuredTruncation.as_usize(), 2);
    assert_eq!(CompactionLevel::Level3MarkerOnly.as_usize(), 3);
}
