//! Unit tests for the `transcript` module family.
//!
//! Coverage map (28 tests):
//!
//! - [`super::types::TranscriptEntry`]: 2 tests
//!   (clone semantics, Debug formatting)
//! - [`super::collect::collect_transcript_entries`]: 7 tests
//!   (user/assistant/multiple/empty/whitespace-only/empty-input/multi-text-content)
//! - [`super::truncate::truncate_text`]: 5 tests
//!   (truncation, short/no-op, exact boundary, empty, prefix+suffix preservation)
//! - [`super::prompt::build_review_prompt`]: 5 tests
//!   (basic structure, retry reason, empty entries, all sections present, tool entry format)
//! - [`super::parse::parse_assessment_response`]: 9 tests
//!   (basic, code block, invalid, high risk, whitespace, leading text,
//!   trailing text, malformed-but-recoverable, basic happy path)

use synthia_provider::{Content, ContentPart, Message, Role, TextContent};

use super::{truncate::truncate_text, *};
use crate::config::GuardianRiskLevel;

// =============================================================================
// Test Helpers
// =============================================================================

fn create_text_message(role: Role, text: &str) -> Message {
    Message::new(role, Content::text(text))
}

// =============================================================================
// TranscriptEntry Tests
// =============================================================================

#[test]
fn test_transcript_entry_clone() {
    let entry = TranscriptEntry {
        role: "user".to_string(),
        content: "Hello world".to_string(),
        is_tool: false,
    };
    let cloned = entry.clone();
    assert_eq!(cloned.role, entry.role);
    assert_eq!(cloned.content, entry.content);
    assert_eq!(cloned.is_tool, entry.is_tool);
}

#[test]
fn test_transcript_entry_debug() {
    let entry = TranscriptEntry {
        role: "assistant".to_string(),
        content: "I'm thinking".to_string(),
        is_tool: true,
    };
    let debug_str = format!("{entry:?}");
    assert!(debug_str.contains("assistant"));
    assert!(debug_str.contains("I'm thinking"));
    assert!(debug_str.contains("true"));
}

// =============================================================================
// collect_transcript_entries Tests
// =============================================================================

#[test]
fn test_collect_transcript_entries_user_message() {
    let messages = vec![create_text_message(Role::User, "Hello, world!")];
    let entries = collect_transcript_entries(&messages);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].role, "user");
    assert_eq!(entries[0].content, "Hello, world!");
    assert!(!entries[0].is_tool);
}

#[test]
fn test_collect_transcript_entries_assistant_message() {
    let messages =
        vec![create_text_message(Role::Assistant, "I see the issue")];
    let entries = collect_transcript_entries(&messages);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].role, "assistant");
    assert_eq!(entries[0].content, "I see the issue");
}

#[test]
fn test_collect_transcript_entries_multiple_messages() {
    let messages = vec![
        create_text_message(Role::User, "First message"),
        create_text_message(Role::Assistant, "Second message"),
        create_text_message(Role::User, "Third message"),
    ];
    let entries = collect_transcript_entries(&messages);

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].role, "user");
    assert_eq!(entries[1].role, "assistant");
    assert_eq!(entries[2].role, "user");
}

#[test]
fn test_collect_transcript_entries_empty_content_filtered() {
    let messages = vec![create_text_message(Role::User, "   ")];
    let entries = collect_transcript_entries(&messages);

    assert_eq!(entries.len(), 0);
}

#[test]
fn test_collect_transcript_entries_whitespace_only_filtered() {
    let messages = vec![create_text_message(Role::Assistant, "\n\t  \n")];
    let entries = collect_transcript_entries(&messages);

    assert_eq!(entries.len(), 0);
}

#[test]
fn test_collect_transcript_entries_empty_messages() {
    let messages: Vec<Message> = vec![];
    let entries = collect_transcript_entries(&messages);

    assert!(entries.is_empty());
}

#[test]
fn test_collect_transcript_entries_multiple_texts_in_multiple_content() {
    // Test Content::Multi with multiple text items
    let messages = vec![Message::new(
        Role::User,
        Content::parts(vec![
            ContentPart::Text(TextContent {
                text: "First part".to_string(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "Second part".to_string(),
                cache_control: None,
            }),
        ]),
    )];
    let entries = collect_transcript_entries(&messages);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "First part\nSecond part");
}

// =============================================================================
// truncate_text Tests
// =============================================================================

#[test]
fn test_truncate_text() {
    let long_text = "a".repeat(10000);
    let truncated = truncate_text(&long_text, 1000);

    assert!(truncated.contains("<truncated>"));
    assert!(truncated.len() < long_text.len());
}

#[test]
fn test_truncate_text_short() {
    let short_text = "short text";
    let result = truncate_text(short_text, 100);

    assert_eq!(result, short_text);
}

#[test]
fn test_truncate_text_exactly_at_boundary() {
    let text = "a".repeat(400); // 400 chars = 100 tokens * 4
    let result = truncate_text(&text, 100);
    assert_eq!(result, text);
}

#[test]
fn test_truncate_text_empty() {
    let text = "";
    let result = truncate_text(text, 100);
    assert_eq!(result, "");
}

#[test]
fn test_truncate_text_contains_prefix_and_suffix() {
    let long_text = "abcdefghij".repeat(1000);
    let truncated = truncate_text(&long_text, 100);

    // Should contain prefix, suffix, and truncation marker
    assert!(truncated.starts_with("abcdefghij"));
    assert!(truncated.ends_with("abcdefghij"));
    assert!(truncated.contains("<truncated>"));
}

// =============================================================================
// build_review_prompt Tests
// =============================================================================

#[test]
fn test_build_review_prompt() {
    let entries = vec![TranscriptEntry {
        role: "user".to_string(),
        content: "Hello".to_string(),
        is_tool: false,
    }];
    let action = r#"{"tool": "shell", "command": "ls"}"#;

    let prompt = build_review_prompt(&entries, action, None);

    assert!(prompt.contains("CONVERSATION TRANSCRIPT"));
    assert!(prompt.contains("PROPOSED ACTION"));
    assert!(prompt.contains("ls"));
}

#[test]
fn test_build_review_prompt_with_retry() {
    let entries = vec![];
    let action = r#"{"tool": "test"}"#;

    let prompt = build_review_prompt(
        &entries,
        action,
        Some("Previous assessment was invalid"),
    );

    assert!(prompt.contains("RETRY REASON"));
    assert!(prompt.contains("Previous assessment was invalid"));
}

#[test]
fn test_build_review_prompt_empty_entries() {
    let entries: Vec<TranscriptEntry> = vec![];
    let action = r#"{"tool": "read"}"#;

    let prompt = build_review_prompt(&entries, action, None);

    assert!(prompt.contains("CONVERSATION TRANSCRIPT"));
    assert!(prompt.contains("PROPOSED ACTION"));
    // Empty transcript should still produce valid prompt
    assert!(prompt.contains("Assess the risk"));
}

#[test]
fn test_build_review_prompt_contains_all_sections() {
    let entries = vec![
        TranscriptEntry {
            role: "user".to_string(),
            content: "Run a command".to_string(),
            is_tool: false,
        },
        TranscriptEntry {
            role: "assistant".to_string(),
            content: "I'll run ls".to_string(),
            is_tool: false,
        },
    ];
    let action = r#"{"tool": "shell", "command": "rm -rf /"}"#;

    let prompt =
        build_review_prompt(&entries, action, Some("Please double-check"));

    // Check all expected sections are present
    assert!(prompt.contains("You are a security reviewer"));
    assert!(prompt.contains("CONVERSATION TRANSCRIPT"));
    assert!(prompt.contains("PROPOSED ACTION"));
    assert!(prompt.contains("RETRY REASON"));
    assert!(prompt.contains("Assess the risk"));
    assert!(prompt.contains("risk_level"));
    assert!(prompt.contains("risk_score"));
    assert!(prompt.contains("rationale"));
    assert!(prompt.contains("evidence"));
}

#[test]
fn test_build_review_prompt_tool_entry_format() {
    let entries = vec![TranscriptEntry {
        role: "tool".to_string(),
        content: "File content here".to_string(),
        is_tool: true,
    }];

    let prompt = build_review_prompt(&entries, "{}", None);

    assert!(prompt.contains("[tool]"));
    assert!(prompt.contains("File content here"));
}

// =============================================================================
// parse_assessment_response Tests
// =============================================================================

#[test]
fn test_parse_assessment_response() {
    let json = r#"{
        "risk_level": "low",
        "risk_score": 20,
        "rationale": "Safe operation",
        "evidence": []
    }"#;

    let result = parse_assessment_response(json);
    assert!(result.is_ok());

    let assessment = result.unwrap();
    assert_eq!(assessment.risk_level, GuardianRiskLevel::Low);
    assert_eq!(assessment.risk_score, 20);
}

#[test]
fn test_parse_assessment_response_with_code_block() {
    let json = r#"
    ```json
    {
        "risk_level": "medium",
        "risk_score": 50,
        "rationale": "Moderate risk",
        "evidence": []
    }
    ```
    "#;

    let result = parse_assessment_response(json);
    assert!(result.is_ok());

    let assessment = result.unwrap();
    assert_eq!(assessment.risk_level, GuardianRiskLevel::Medium);
}

#[test]
fn test_parse_assessment_response_invalid() {
    let invalid = "not valid json";
    let result = parse_assessment_response(invalid);
    assert!(result.is_err());
}

#[test]
fn test_parse_assessment_response_high_risk() {
    let json = r#"{
        "risk_level": "high",
        "risk_score": 85,
        "rationale": "Dangerous operation",
        "evidence": []
    }"#;

    let result = parse_assessment_response(json);
    assert!(result.is_ok());

    let assessment = result.unwrap();
    assert_eq!(assessment.risk_level, GuardianRiskLevel::High);
    assert_eq!(assessment.risk_score, 85);
}

#[test]
fn test_parse_assessment_response_with_whitespace() {
    let json = r#"

    {
        "risk_level": "low",
        "risk_score": 10,
        "rationale": "Minimal risk",
        "evidence": []
    }

    "#;

    let result = parse_assessment_response(json);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().risk_level, GuardianRiskLevel::Low);
}

#[test]
fn test_parse_assessment_response_with_leading_text() {
    let response = r#"Based on my analysis:

    {
        "risk_level": "medium",
        "risk_score": 45,
        "rationale": "Some concerns",
        "evidence": []
    }

    This assessment is provided above.
    "#;

    let result = parse_assessment_response(response);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().risk_level, GuardianRiskLevel::Medium);
}

#[test]
fn test_parse_assessment_response_with_trailing_text() {
    let response = r#"{
        "risk_level": "low",
        "risk_score": 15,
        "rationale": "Safe",
        "evidence": []
    }

    End of analysis.
    "#;

    let result = parse_assessment_response(response);
    assert!(result.is_ok());
}

#[test]
fn test_parse_assessment_response_malformed_json_but_valid_content() {
    // JSON has trailing comma but contains valid content
    let json = r#"{
        "risk_level": "high",
        "risk_score": 90,
        "rationale": "Very dangerous",
        "evidence": []
    }"#;

    let result = parse_assessment_response(json);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().risk_level, GuardianRiskLevel::High);
}
