use std::path::PathBuf;

use synthia_provider::types::Message;

use crate::types::{DispatchMode, ToolExecutionContext, ToolInput, ToolOutput};

#[test]
fn dispatch_mode_variants() {
    assert!(matches!(DispatchMode::Fork, DispatchMode::Fork));
    assert!(matches!(DispatchMode::Teammate, DispatchMode::Teammate));
    assert!(matches!(DispatchMode::Worktree, DispatchMode::Worktree));
}

#[test]
fn dispatch_mode_serialize_deserialize() {
    let mode = DispatchMode::Teammate;
    let json = serde_json::to_string(&mode).unwrap();
    let parsed: DispatchMode = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, mode);
}

#[test]
fn tool_execution_context_new() {
    let ctx = ToolExecutionContext::new(
        "session-123".to_string(),
        PathBuf::from("/workspace"),
    );
    assert_eq!(ctx.session_id, "session-123");
    assert_eq!(ctx.workspace_root, PathBuf::from("/workspace"));
    assert_eq!(ctx.caller_agent, "default");
    assert_eq!(ctx.dispatch_mode, DispatchMode::Fork);
    assert!(ctx.messages.is_empty());
}

#[test]
fn tool_execution_context_with_messages() {
    let msg = Message::user("hello");
    let ctx = ToolExecutionContext::new(
        "session-123".to_string(),
        PathBuf::from("/workspace"),
    )
    .with_messages(vec![msg.clone()]);
    assert_eq!(ctx.messages.len(), 1);
}

#[test]
fn tool_input_structure() {
    let ctx = ToolExecutionContext::new(
        "session-123".to_string(),
        PathBuf::from("/workspace"),
    );
    let input = ToolInput {
        name: "test_tool".to_string(),
        input: serde_json::json!({"arg": "value"}),
        context: ctx,
    };
    assert_eq!(input.name, "test_tool");
    assert_eq!(input.input["arg"], "value");
}

#[test]
fn tool_output_text() {
    let output = ToolOutput::text("hello world");
    assert!(output.is_text());
    assert!(output.is_error.is_none());
}

#[test]
fn tool_output_error() {
    let output = ToolOutput::error("something went wrong");
    assert!(!output.is_text());
    assert_eq!(output.is_error, Some(true));
}

#[test]
fn tool_output_from_string() {
    let output: ToolOutput = "hello".to_string().into();
    assert!(output.is_text());
}

#[test]
fn tool_output_clone() {
    let output = ToolOutput::text("hello");
    let cloned = output.clone();
    assert!(cloned.is_text());
    assert_eq!(cloned.content.len(), 1);
}
