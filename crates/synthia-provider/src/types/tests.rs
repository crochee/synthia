//! Unit tests for the `types` module family.
//!
//! Coverage map (8 tests):
//!
//! - [`stream_chunk_tests`]: 4 tests covering the 3 tool-call
//!   streaming variants
//!   ([`stream_chunk_tests::test_tool_call_start_variant`],
//!   [`stream_chunk_tests::test_tool_call_delta_variant`],
//!   [`stream_chunk_tests::test_tool_call_end_variant`])
//!   plus backward-compat for the `From<ContentPart>` conversion
//!   ([`stream_chunk_tests::test_content_backward_compat`]).
//! - [`tool_result_cleared_at_tests`]: 4 tests pinning the
//!   `tool_result_cleared_at` P8 contract — default-is-None
//!   ([`tool_result_cleared_at_tests::new_message_has_field_as_none_by_default`]),
//!   legacy-JSON deserializes-without-the-field
//!   ([`tool_result_cleared_at_tests::old_json_without_field_deserializes_as_none`]),
//!   round-trip when set
//!   ([`tool_result_cleared_at_tests::new_json_with_field_round_trips`]),
//!   `skip_serializing_if = "Option::is_none"` omits the field
//!   ([`tool_result_cleared_at_tests::skip_serializing_if_none_omits_field_in_json`]).

use chrono::{DateTime, Utc};

use super::*;

#[cfg(test)]
mod stream_chunk_tests {
    use super::*;

    #[test]
    fn test_tool_call_start_variant() {
        let chunk = StreamChunk::ToolCallStart {
            id: "call-1".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path": "/tmp/test"}),
        };
        match &chunk {
            StreamChunk::ToolCallStart {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(name, "read_file");
                assert_eq!(arguments["path"], "/tmp/test");
            }
            _ => panic!("Expected ToolCallStart"),
        }
    }

    #[test]
    fn test_tool_call_delta_variant() {
        let chunk = StreamChunk::ToolCallDelta {
            id: "call-1".to_string(),
            arguments_delta: r#"{"path": "/tm"#.to_string(),
        };
        match &chunk {
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(arguments_delta, r#"{"path": "/tm"#);
            }
            _ => panic!("Expected ToolCallDelta"),
        }
    }

    #[test]
    fn test_tool_call_end_variant() {
        let chunk = StreamChunk::ToolCallEnd {
            id: "call-1".to_string(),
        };
        match &chunk {
            StreamChunk::ToolCallEnd { id } => {
                assert_eq!(id, "call-1");
            }
            _ => panic!("Expected ToolCallEnd"),
        }
    }

    #[test]
    fn test_content_backward_compat() {
        let text_part = ContentPart::Text(TextContent {
            text: "hello".to_string(),
            cache_control: None,
        });
        let chunk: StreamChunk = text_part.into();
        match &chunk {
            StreamChunk::Content(ContentPart::Text(tc)) => {
                assert_eq!(tc.text, "hello");
            }
            _ => panic!("Expected Content variant"),
        }
    }
}

#[cfg(test)]
mod tool_result_cleared_at_tests {
    use super::*;

    #[test]
    fn new_message_has_field_as_none_by_default() {
        // The field is None on a freshly-built Message — both via the
        // `new` constructor and via `Default::default()`.
        let from_new = Message::user("hi");
        assert!(from_new.tool_result_cleared_at.is_none());

        let from_default = Message::default();
        assert!(from_default.tool_result_cleared_at.is_none());
    }

    #[test]
    fn old_json_without_field_deserializes_as_none() {
        // Backward-compat invariant: pre-change on-disk messages
        // (lacking the field) MUST still deserialize, with the field
        // defaulting to None. We use the actual on-the-wire shape
        // (externally-tagged `Content` enum) rather than the
        // string-coerced `From<&str>` form, so this test pins the
        // real serialization format.
        let legacy_json = r#"{
            "role": "user",
            "content": {"Single": {"type": "text", "text": "hello"}}
        }"#;
        let m: Message = serde_json::from_str(legacy_json)
            .expect("legacy JSON must deserialize");
        assert!(m.tool_result_cleared_at.is_none());
        assert_eq!(m.role, Role::User);
    }

    #[test]
    fn new_json_with_field_round_trips() {
        // Set the field, serialize, deserialize — the value survives.
        let ts = DateTime::parse_from_rfc3339("2026-06-12T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let original = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "the result".to_string(),
                cache_control: None,
            })),
            tool_call_id: Some("call-1".to_string()),
            name: None,
            tool_result_cleared_at: Some(ts),
        };
        let json = serde_json::to_string(&original)
            .expect("serialization must succeed");
        let restored: Message =
            serde_json::from_str(&json).expect("round-trip must succeed");
        assert_eq!(restored.tool_result_cleared_at, Some(ts));
        assert_eq!(restored.tool_call_id.as_deref(), Some("call-1"));
    }

    #[test]
    fn skip_serializing_if_none_omits_field_in_json() {
        // The serde attribute `skip_serializing_if = "Option::is_none"`
        // keeps the JSON payload small and stable when nothing is set.
        let m = Message::user("hi");
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            !json.contains("tool_result_cleared_at"),
            "field must be omitted when None, got: {json}"
        );
    }
}

#[cfg(test)]
mod message_kind_and_llm_visible_tests {
    use super::*;

    #[test]
    fn message_kind_has_five_variants() {
        let _ = vec![
            MessageKind::System,
            MessageKind::User,
            MessageKind::Assistant,
            MessageKind::ToolCall,
            MessageKind::ToolResult,
        ];
    }

    #[test]
    fn user_message_is_llm_visible() {
        let msg = Message::user("hi");
        assert!(msg.llm_visible());
        assert_eq!(msg.kind(), MessageKind::User);
    }

    #[test]
    fn system_message_is_llm_visible() {
        let msg = Message::system("instructions");
        assert!(msg.llm_visible());
        assert_eq!(msg.kind(), MessageKind::System);
    }

    #[test]
    fn assistant_message_is_llm_visible() {
        let msg = Message::assistant("response");
        assert!(msg.llm_visible());
        assert_eq!(msg.kind(), MessageKind::Assistant);
    }

    #[test]
    fn assistant_with_tool_use_is_tool_call_kind() {
        let msg = Message {
            role: Role::Assistant,
            content: Content::Multi(vec![ContentPart::ToolUse(ToolUse {
                id: "call-1".to_string(),
                name: "read".to_string(),
                input: serde_json::json!({}),
            })]),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        assert_eq!(msg.kind(), MessageKind::ToolCall);
        assert!(msg.llm_visible());
    }

    #[test]
    fn tool_result_with_content_is_llm_visible() {
        let msg = Message::tool(Content::text("result"), "call-1");
        assert!(msg.llm_visible());
        assert_eq!(msg.kind(), MessageKind::ToolResult);
    }

    #[test]
    fn tool_result_with_empty_content_is_not_llm_visible() {
        let msg = Message::tool(Content::text(""), "call-1");
        assert!(!msg.llm_visible());
        assert_eq!(msg.kind(), MessageKind::ToolResult);
    }

    #[test]
    fn from_role_maps_correctly() {
        assert_eq!(
            MessageKind::from_role(Role::System, false),
            MessageKind::System
        );
        assert_eq!(
            MessageKind::from_role(Role::User, false),
            MessageKind::User
        );
        assert_eq!(
            MessageKind::from_role(Role::Assistant, false),
            MessageKind::Assistant
        );
        assert_eq!(
            MessageKind::from_role(Role::Assistant, true),
            MessageKind::ToolCall
        );
        assert_eq!(
            MessageKind::from_role(Role::Tool, false),
            MessageKind::ToolResult
        );
    }

    /// Performance contract: `llm_visible()` is O(1) and side-effect free.
    /// Calling it in a tight loop over 10 000 messages MUST complete in
    /// under 1 ms on a developer workstation.
    #[test]
    fn llm_visible_performance_contract() {
        let msg = Message::user("performance test payload");
        let iterations = 10_000;
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(msg.llm_visible());
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1,
            "llm_visible() over {iterations} calls took {elapsed:?}, expected < 1ms"
        );
    }
}
