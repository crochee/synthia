use std::path::PathBuf;

use synthia_provider::types::Message;

use crate::types::{
    DispatchMode,
    ToolExecutionContext,
    ToolInput,
    ToolOutput,
    TruncatedBy,
};

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

#[test]
fn tool_output_from_raw_wraps_json_as_text() {
    let raw = serde_json::json!({"status": "ok", "data": [1, 2, 3]});
    let output = ToolOutput::from_raw(raw.clone());
    assert!(output.is_text());
    assert!(output.metadata.is_empty());
    assert!(output.truncated_by.is_none());
    // The textual content should contain the JSON serialization.
    assert!(output.content[0].text().unwrap().contains("\"status\""));
}

#[test]
fn tool_output_with_truncated_by_lines() {
    let output = ToolOutput::text("hi").with_truncated_by(TruncatedBy::Lines {
        shown: 2,
        total: 10,
    });
    match output.truncated_by {
        Some(TruncatedBy::Lines { shown, total }) => {
            assert_eq!(shown, 2);
            assert_eq!(total, 10);
        }
        other => panic!("expected Lines truncation, got {other:?}"),
    }
}

#[test]
fn tool_output_with_truncated_by_bytes() {
    let output = ToolOutput::text("hi").with_truncated_by(TruncatedBy::Bytes {
        shown: 50_000,
        total: 1_000_000,
    });
    match output.truncated_by {
        Some(TruncatedBy::Bytes { shown, total }) => {
            assert_eq!(shown, 50_000);
            assert_eq!(total, 1_000_000);
        }
        other => panic!("expected Bytes truncation, got {other:?}"),
    }
}

#[test]
fn tool_output_with_metadata_inserts_entry() {
    let output = ToolOutput::text("ok")
        .with_metadata("line_count", serde_json::json!(42))
        .with_metadata("truncated", serde_json::json!(true));
    assert_eq!(output.metadata["line_count"], serde_json::json!(42));
    assert_eq!(output.metadata["truncated"], serde_json::json!(true));
}

#[test]
fn tool_output_serialize_with_truncation() {
    let output = ToolOutput::text("ok")
        .with_truncated_by(TruncatedBy::Lines { shown: 1, total: 5 });
    let json = serde_json::to_value(&output).unwrap();
    assert_eq!(json["truncated_by"]["kind"], "lines");
    assert_eq!(json["truncated_by"]["shown"], 1);
    assert_eq!(json["truncated_by"]["total"], 5);
    // `truncated_by` is `skip_serializing_if = "Option::is_none"`.
    let output2 = ToolOutput::text("ok");
    let json2 = serde_json::to_value(&output2).unwrap();
    assert!(json2.get("truncated_by").is_none());
}
