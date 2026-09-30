//! TDD Tests for ToolResult API refactoring
//!
//! RED Phase: Write failing tests first

use synthia_provider::{ContentPart, ToolResult};

#[test]
fn test_tool_result_new_with_strings() {
    let result = ToolResult::new("call-123", "Hello, World!");

    assert_eq!(result.tool_use_id, "call-123");
    assert_eq!(result.content.len(), 1);
    match &result.content[0] {
        ContentPart::Text(text) => {
            assert_eq!(text.text, "Hello, World!");
        }
        _ => panic!("Expected Text content part"),
    }
    assert!(result.structured_content.is_none());
    assert!(result.is_error.is_none());
}

#[test]
fn test_tool_result_error_with_strings() {
    let result = ToolResult::error("call-123", "Something went wrong");

    assert_eq!(result.tool_use_id, "call-123");
    assert_eq!(result.content.len(), 1);
    match &result.content[0] {
        ContentPart::Text(text) => {
            assert_eq!(text.text, "Something went wrong");
        }
        _ => panic!("Expected Text content part"),
    }
    assert!(result.structured_content.is_none());
    assert_eq!(result.is_error, Some(true));
}

#[test]
fn test_tool_result_new_with_empty_string() {
    let result = ToolResult::new("call-123", "");

    assert_eq!(result.tool_use_id, "call-123");
    assert_eq!(result.content.len(), 1);
}

#[test]
fn test_tool_result_clone_is_independent() {
    let result = ToolResult::new("call-123", "Original");
    let cloned = result.clone();

    assert_eq!(cloned.tool_use_id, result.tool_use_id);
    assert_eq!(cloned.content.len(), result.content.len());
    assert_eq!(cloned.is_error, result.is_error);
    assert_eq!(cloned.structured_content, result.structured_content);
}
