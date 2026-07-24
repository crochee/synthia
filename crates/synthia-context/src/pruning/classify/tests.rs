//! Tests for message classification, tool-pair repair, and micro-compact.

use synthia_provider::{
    Content,
    ContentPart,
    Message,
    Role,
    TextContent,
    ToolResult,
    ToolUse,
};

use super::*;

fn tool_result(id: &str, content: &str) -> ToolResult {
    ToolResult {
        tool_use_id: id.to_string(),
        content: vec![ContentPart::Text(TextContent {
            text: content.to_string(),
            cache_control: None,
        })],
        structured_content: None,
        is_error: None,
    }
}

fn create_user_text_message(role: Role, text: &str) -> Message {
    Message::new(role, Content::text(text))
}

fn create_tool_use_message(
    role: Role,
    id: &str,
    name: &str,
    args: serde_json::Value,
) -> Message {
    Message::new(
        role,
        Content::Single(ContentPart::ToolUse(ToolUse {
            id: id.to_string(),
            name: name.to_string(),
            input: args,
        })),
    )
}

fn create_tool_result_message(
    role: Role,
    tool_use_id: &str,
    result_text: &str,
) -> Message {
    Message::new(
        role,
        Content::Single(ContentPart::ToolResult(tool_result(
            tool_use_id,
            result_text,
        ))),
    )
}

// =============================================================================
// fix_tool_pairs Tests
// =============================================================================

#[test]
fn test_fix_tool_pairs_with_single_content() {
    let messages = vec![
        Message::new(
            Role::Assistant,
            Content::Single(ContentPart::ToolUse(ToolUse {
                id: "tool-1".to_string(),
                name: "test_tool".to_string(),
                input: serde_json::json!({}),
            })),
        ),
        Message::new(
            Role::User,
            Content::Single(ContentPart::ToolResult(tool_result(
                "tool-1", "result",
            ))),
        ),
    ];

    let fixed = fix_tool_pairs(&messages);
    assert_eq!(fixed.len(), 2);
}

#[test]
fn test_fix_tool_pairs_with_multiple_content() {
    let messages = vec![
        Message::new(
            Role::Assistant,
            Content::Multi(vec![
                ContentPart::ToolUse(ToolUse {
                    id: "tool-1".to_string(),
                    name: "test_tool_1".to_string(),
                    input: serde_json::json!({}),
                }),
                ContentPart::ToolUse(ToolUse {
                    id: "tool-2".to_string(),
                    name: "test_tool_2".to_string(),
                    input: serde_json::json!({}),
                }),
            ]),
        ),
        Message::new(
            Role::User,
            Content::Multi(vec![
                ContentPart::ToolResult(tool_result("tool-1", "result 1")),
                ContentPart::ToolResult(tool_result("tool-2", "result 2")),
            ]),
        ),
    ];

    let fixed = fix_tool_pairs(&messages);
    assert_eq!(fixed.len(), 2);
}

#[test]
fn test_fix_tool_pairs_preserves_text_messages() {
    let messages = vec![
        create_user_text_message(Role::User, "hello"),
        Message::new(
            Role::Assistant,
            Content::Single(ContentPart::ToolUse(ToolUse {
                id: "tool-1".to_string(),
                name: "test_tool".to_string(),
                input: serde_json::json!({}),
            })),
        ),
        Message::new(
            Role::User,
            Content::Single(ContentPart::ToolResult(tool_result(
                "tool-1", "result",
            ))),
        ),
    ];

    let fixed = fix_tool_pairs(&messages);
    assert_eq!(fixed.len(), 3);
    assert!(matches!(
        classify_message(&fixed[0]),
        MessageClassification::UserText
    ));
}

#[test]
fn test_classify_messages_mixed() {
    let messages = vec![
        create_user_text_message(Role::User, "hi"),
        create_tool_use_message(
            Role::Assistant,
            "t1",
            "tool",
            serde_json::json!({}),
        ),
        create_tool_result_message(Role::User, "t1", "r"),
        create_user_text_message(Role::Assistant, "ok"),
    ];
    let classes = classify_messages(&messages);
    assert_eq!(
        classes,
        vec![
            MessageClassification::UserText,
            MessageClassification::ToolUse,
            MessageClassification::ToolResult,
            MessageClassification::AssistantText,
        ]
    );
}

#[test]
fn test_is_helpers() {
    let tool_use = create_tool_use_message(
        Role::Assistant,
        "t1",
        "tool",
        serde_json::json!({}),
    );
    let tool_result = create_tool_result_message(Role::User, "t1", "r");
    let user_text = create_user_text_message(Role::User, "hi");

    assert!(is_tool_use(&tool_use));
    assert!(!is_tool_use(&user_text));

    assert!(is_tool_result(&tool_result));
    assert!(!is_tool_result(&user_text));

    assert!(is_user_text_message(&user_text));
    assert!(!is_user_text_message(&tool_result));
}

#[test]
fn test_get_tool_ids() {
    let tool_use = create_tool_use_message(
        Role::Assistant,
        "t1",
        "tool",
        serde_json::json!({}),
    );
    let tool_result = create_tool_result_message(Role::User, "t1", "r");

    assert_eq!(get_tool_use_id(&tool_use), Some("t1".to_string()));
    assert_eq!(get_tool_result_id(&tool_result), Some("t1".to_string()));
}

#[test]
fn test_find_tool_use_for_result_and_back() {
    let messages = vec![
        create_tool_use_message(
            Role::Assistant,
            "t1",
            "tool",
            serde_json::json!({}),
        ),
        create_tool_result_message(Role::User, "t1", "r"),
    ];
    assert_eq!(find_tool_use_for_result(&messages, "t1"), Some(0));
    assert_eq!(find_result_for_tool_use(&messages, "t1"), Some(1));
    assert_eq!(find_tool_use_for_result(&messages, "missing"), None);
}

#[test]
fn test_micro_compact_keeps_recent() {
    let mut messages = vec![
        create_tool_result_message(Role::User, "t1", "old-1"),
        create_tool_result_message(Role::User, "t2", "old-2"),
        create_tool_result_message(Role::User, "t3", "new-1"),
        create_tool_result_message(Role::User, "t4", "new-2"),
    ];
    micro_compact(&mut messages, 2);
    // The two oldest should be cleared (Text "[cleared]"), two newest retained (ToolResult).
    assert_eq!(
        messages[0].content.extract_text().as_deref(),
        Some("[cleared]")
    );
    assert_eq!(
        messages[1].content.extract_text().as_deref(),
        Some("[cleared]")
    );
    let ContentPart::ToolResult(tr2) =
        (&messages[2].content).into_iter().next().unwrap()
    else {
        panic!("expected ToolResult at index 2");
    };
    assert_eq!(tr2.content[0].text().unwrap(), "new-1");
    let ContentPart::ToolResult(tr3) =
        (&messages[3].content).into_iter().next().unwrap()
    else {
        panic!("expected ToolResult at index 3");
    };
    assert_eq!(tr3.content[0].text().unwrap(), "new-2");
}

#[test]
fn test_micro_compact_no_op_when_under_threshold() {
    let mut messages = vec![create_tool_result_message(Role::User, "t1", "r")];
    micro_compact(&mut messages, 5);
    // No change: still a ToolResult with the original text.
    let ContentPart::ToolResult(tr) =
        (&messages[0].content).into_iter().next().unwrap()
    else {
        panic!("expected ToolResult");
    };
    assert_eq!(tr.content[0].text().unwrap(), "r");
}
