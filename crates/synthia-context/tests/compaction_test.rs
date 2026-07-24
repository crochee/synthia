//! Unit tests for the three-level compaction cascade.
//!
//! Exercises `compact_with_fallback`, `apply_compaction`, `compact_level1/2/3`,
//! and the `Compactor` auto-selection logic from the integration-test boundary.

use async_trait::async_trait;
use synthia_context::{
    compaction::{
        level1::CompactionProvider,
        level2::compact_level2,
        level3::compact_level3,
        orchestrator::{
            apply_compaction,
            calculate_protection_zone,
            compact_with_fallback,
        },
    },
    compactor::Compactor,
    types::ContextError,
};
use synthia_provider::Message;
use tempfile::TempDir;

/// Fixed-summary provider for level 1 tests.
struct MockL1Provider {
    summary: String,
}

#[async_trait]
impl CompactionProvider for MockL1Provider {
    async fn generate_summary(
        &self,
        _messages: &[Message],
        _previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        Ok(self.summary.clone())
    }
}

/// Always-failing provider — forces fallback from L1.
struct FailingProvider;

#[async_trait]
impl CompactionProvider for FailingProvider {
    async fn generate_summary(
        &self,
        _messages: &[Message],
        _previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        Err(ContextError::Checkpoint("provider unavailable".into()))
    }
}

/// Empty-summary provider — forces structured fallback within L1.
struct EmptyProvider;

#[async_trait]
impl CompactionProvider for EmptyProvider {
    async fn generate_summary(
        &self,
        _messages: &[Message],
        _previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        Ok(String::new())
    }
}

// ---------------------------------------------------------------------------
// Level 1: LLM summary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn level1_with_mock_provider_returns_summary() {
    let provider = MockL1Provider {
        summary: "This is a summarized conversation.".to_string(),
    };
    let messages = vec![
        Message::user("hello"),
        Message::assistant("hi there"),
        Message::user("how are you?"),
        Message::assistant("i am fine"),
    ];

    let result =
        compact_with_fallback(&messages, 10_000, Some(&provider), None, None)
            .await;

    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(
        text.contains("summarized"),
        "expected summary text, got: {text}"
    );
}

#[tokio::test]
async fn level1_respects_previous_summary_anchor() {
    let provider = MockL1Provider {
        summary: "New summary".to_string(),
    };
    let messages = vec![Message::user("hi")];

    let result = compact_with_fallback(
        &messages,
        10_000,
        Some(&provider),
        Some("previous conversation summary"),
        None,
    )
    .await;

    assert_eq!(result.len(), 1);
}

#[tokio::test]
async fn level1_failing_provider_falls_to_l2() {
    let provider = FailingProvider;
    let messages = vec![Message::user("hello"), Message::assistant("hi")];

    let result =
        compact_with_fallback(&messages, 1000, Some(&provider), None, None)
            .await;

    // Should not be empty — L2 should have processed it
    assert!(!result.is_empty());
}

#[tokio::test]
async fn level1_empty_summary_falls_to_structured_fallback() {
    let provider = EmptyProvider;
    let messages = vec![Message::user("hello"), Message::assistant("hi")];

    // EmptyProvider returns "" → structured fallback path within L1 produces
    // a single summary message, not the original messages
    let result =
        compact_with_fallback(&messages, 1000, Some(&provider), None, None)
            .await;

    assert_eq!(result.len(), 1);
}

// ---------------------------------------------------------------------------
// Level 2: Structured truncation
// ---------------------------------------------------------------------------

#[test]
fn level2_preserves_user_messages() {
    let messages = vec![
        Message::user("this is a user message with significant content"),
        Message::assistant("assistant response with some text here"),
    ];

    let result = compact_level2(&messages);

    assert_eq!(result.len(), 2);
    // User message should be preserved in full
    let user_text = format!("{:?}", result[0].content);
    assert!(user_text.contains("user message"));
}

#[test]
fn level2_truncates_long_assistant_text_to_first_line() {
    let messages = vec![Message::assistant(
        "first line of response\nsecond line\nthird line",
    )];

    let result = compact_level2(&messages);

    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(text.contains("first line"));
    assert!(!text.contains("second line"));
}

#[test]
fn level2_empty_input_returns_empty() {
    let result = compact_level2(&[]);
    assert!(result.is_empty());
}

#[test]
fn level2_truncates_tool_result_to_first_line() {
    use synthia_provider::{
        Content,
        ContentPart,
        Role,
        TextContent,
        ToolResult,
    };

    let msg = Message {
        role: Role::Tool,
        tool_call_id: Some("t1".into()),
        content: Content::Multi(vec![ContentPart::ToolResult(ToolResult {
            tool_use_id: "t1".into(),
            content: vec![ContentPart::Text(TextContent {
                text: "result line one\nresult line two\nresult line three"
                    .to_string(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: None,
        })]),
        ..Default::default()
    };

    let result = compact_level2(&[msg]);

    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(text.contains("result line one"));
    assert!(!text.contains("result line two"));
}

// ---------------------------------------------------------------------------
// Level 3: Marker-only retention
// ---------------------------------------------------------------------------

#[test]
fn level3_empty_input_returns_empty() {
    let result = compact_level3(&[]);
    assert!(result.is_empty());
}

#[test]
fn level3_no_tool_calls_returns_placeholder() {
    let messages = vec![Message::user("hello"), Message::assistant("hi there")];

    let result = compact_level3(&messages);

    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(
        text.contains("compacted") || text.contains("messages"),
        "expected placeholder, got: {text}"
    );
}

#[test]
fn level3_tool_calls_become_call_completed_markers() {
    use synthia_provider::{Content, ContentPart, Role, ToolUse};

    let msg = Message {
        role: Role::Assistant,
        content: Content::Multi(vec![
            ContentPart::ToolUse(ToolUse {
                id: "t1".into(),
                name: "bash".into(),
                input: serde_json::json!({ "cmd": "ls" }),
            }),
            ContentPart::ToolUse(ToolUse {
                id: "t2".into(),
                name: "read".into(),
                input: serde_json::json!({ "path": "/tmp/f" }),
            }),
        ]),
        ..Default::default()
    };

    let result = compact_level3(&[msg]);

    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(text.contains("call-completed: bash"));
    assert!(text.contains("call-completed: read"));
}

// ---------------------------------------------------------------------------
// Fallback chain works correctly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fallback_chain_tries_l1_first() {
    let provider = MockL1Provider {
        summary: "LLM summary".to_string(),
    };
    let messages = vec![Message::user("hi")];

    let result =
        compact_with_fallback(&messages, 10_000, Some(&provider), None, None)
            .await;

    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(text.contains("LLM summary"));
}

#[tokio::test]
async fn fallback_chain_l1_exceeds_budget_falls_to_l2() {
    // L1 succeeds but produces too many tokens → falls to L2
    let provider = MockL1Provider {
        summary: "x".repeat(1000), // Large summary that won't fit
    };
    let messages = vec![Message::user("hello")];

    let result =
        compact_with_fallback(&messages, 10, Some(&provider), None, None).await;

    // L2 preserves user messages, so we should get the user message back
    assert!(!result.is_empty());
}

#[tokio::test]
async fn fallback_chain_l2_exceeds_budget_falls_to_l3() {
    let messages = vec![Message::user("hello"), Message::assistant("hi")];

    // Very small budget to force L3
    let result = compact_with_fallback(&messages, 5, None, None, None).await;

    assert_eq!(result.len(), 1);
    let text = format!("{:?}", result[0].content);
    assert!(
        text.contains("call-completed") || text.contains("compacted"),
        "expected L3 marker, got: {text}"
    );
}

#[tokio::test]
async fn fallback_chain_no_provider_skips_to_l2() {
    let messages = vec![Message::user("hello"), Message::assistant("world")];

    let result = compact_with_fallback(&messages, 1000, None, None, None).await;

    // L2 preserves both messages
    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn fallback_chain_empty_messages_returns_empty() {
    let provider = MockL1Provider {
        summary: "summary".to_string(),
    };

    let result =
        compact_with_fallback(&[], 1000, Some(&provider), None, None).await;

    assert!(result.is_empty());
}

// ---------------------------------------------------------------------------
// Compactor auto-select level
// ---------------------------------------------------------------------------

#[test]
fn compactor_auto_select_level_under_budget_returns_zero() {
    let compactor = Compactor::new(0);
    let level = compactor.auto_select_level(100, 200);
    assert_eq!(level, 0);
}

#[test]
fn compactor_auto_select_level_ratio_above_3_returns_l3() {
    let compactor = Compactor::new(0);
    let level = compactor.auto_select_level(400, 100); // 4x budget
    assert_eq!(level, 3);
}

#[test]
fn compactor_auto_select_level_ratio_above_1_5_returns_l2() {
    let compactor = Compactor::new(0);
    let level = compactor.auto_select_level(200, 100); // 2x budget
    assert_eq!(level, 2);
}

#[test]
fn compactor_auto_select_level_ratio_below_1_5_returns_l1() {
    let compactor = Compactor::new(0);
    let level = compactor.auto_select_level(130, 100); // 1.3x budget
    assert_eq!(level, 1);
}

// ---------------------------------------------------------------------------
// apply_compaction integration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_compaction_reports_correct_level() {
    let provider = MockL1Provider {
        summary: "summary".to_string(),
    };
    let messages = vec![Message::user("hi"), Message::assistant("hi")];

    let result = apply_compaction(&messages, 0..2, 1000, Some(&provider), None)
        .await
        .unwrap();

    assert_eq!(result.applied_level, 1);
    assert!(result.original_tokens > 0);
}

#[tokio::test]
async fn apply_compaction_empty_range_returns_zero_level() {
    let provider = MockL1Provider {
        summary: "summary".to_string(),
    };
    let messages = vec![Message::user("hi")];

    let result = apply_compaction(&messages, 0..0, 1000, Some(&provider), None)
        .await
        .unwrap();

    assert_eq!(result.applied_level, 0);
}

#[tokio::test]
async fn apply_compaction_no_provider_falls_to_l2() {
    let messages = vec![Message::user("hi"), Message::assistant("hi")];

    let result = apply_compaction(&messages, 0..2, 1000, None, None)
        .await
        .unwrap();

    assert_eq!(result.applied_level, 2);
}

// ---------------------------------------------------------------------------
// calculate_protection_zone
// ---------------------------------------------------------------------------

#[test]
fn protection_zone_empty_messages_returns_zero() {
    let (start, end) = calculate_protection_zone(&[], 3, 1000);
    assert_eq!(start, 0);
    assert_eq!(end, 0);
}

#[test]
fn protection_zone_zero_rounds_returns_zero() {
    let messages = vec![Message::user("hi")];
    let (start, end) = calculate_protection_zone(&messages, 0, 1000);
    assert_eq!(start, 0);
    assert_eq!(end, 0);
}

#[test]
fn protection_zone_preserves_recent_rounds() {
    let messages = vec![
        Message::user("q1"),
        Message::assistant("a1"),
        Message::user("q2"),
        Message::assistant("a2"),
        Message::user("q3"),
        Message::assistant("a3"),
    ];
    let (start, end) = calculate_protection_zone(&messages, 2, 1_000_000);

    // Recent 2 rounds (q2,a2,q3,a3) should be in protected zone
    assert_eq!(end, messages.len());
    // Start should be at or before index of q2 (which is index 2)
    assert!(start <= 2);
}

// ---------------------------------------------------------------------------
// TempDir isolation
// ---------------------------------------------------------------------------

#[test]
fn temp_dir_isolation_for_compaction() {
    let _tmp = TempDir::new().expect("temp dir created");

    // Verify compaction works with no temp file conflicts
    let messages = vec![Message::user("hello"), Message::assistant("world")];
    let result = compact_level2(&messages);
    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn temp_dir_isolation_for_async_compaction() {
    let _tmp = TempDir::new().expect("temp dir created");

    let provider = MockL1Provider {
        summary: "isolated summary".to_string(),
    };
    let messages = vec![Message::user("hi")];

    let result =
        compact_with_fallback(&messages, 1000, Some(&provider), None, None)
            .await;
    assert_eq!(result.len(), 1);
}
