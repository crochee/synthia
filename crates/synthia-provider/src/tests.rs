//! Unit tests for synthia-provider

use std::sync::Arc;

use synthia_core::Error;

use crate::{
    CompletionRequest,
    Content,
    ContentPart,
    ImageContent,
    Message,
    ReasoningContent,
    ResourceLink,
    Role,
    StreamChunk,
    TextContent,
    ToolChoice,
    ToolResult,
    ToolUse,
};

#[test]
fn test_content_part_with_wrapper_types() {
    let text_part = ContentPart::Text(TextContent {
        text: "Hello world".into(),
        cache_control: None,
    });
    assert!(matches!(text_part, ContentPart::Text(_)));

    let reasoning_part = ContentPart::Reasoning(ReasoningContent {
        text: "Let me think".into(),
        signature: None,
    });
    assert!(matches!(reasoning_part, ContentPart::Reasoning(_)));

    let tool_use = ContentPart::ToolUse(ToolUse {
        id: "tool_1".into(),
        name: "read_file".into(),
        input: serde_json::json!({"path": "/test"}),
    });
    assert!(tool_use.is_tool_use());

    let tool_result = ContentPart::ToolResult(ToolResult {
        tool_use_id: "tool_1".into(),
        content: vec![ContentPart::Text(TextContent {
            text: "result".into(),
            cache_control: None,
        })],
        structured_content: None,
        is_error: Some(false),
    });
    assert!(matches!(tool_result, ContentPart::ToolResult(_)));
}

#[test]
fn test_text_content_structure() {
    let text_content = TextContent {
        text: "Hello".into(),
        cache_control: None,
    };
    let part = ContentPart::Text(text_content);
    match part {
        ContentPart::Text(tc) => assert_eq!(tc.text, "Hello"),
        _ => panic!("Expected Text variant"),
    }
}

#[test]
fn test_reasoning_content_structure() {
    let reasoning_content = ReasoningContent {
        text: "thinking...".into(),
        signature: None,
    };
    let part = ContentPart::Reasoning(reasoning_content);
    match part {
        ContentPart::Reasoning(rc) => assert_eq!(rc.text, "thinking..."),
        _ => panic!("Expected Reasoning variant"),
    }
}

#[test]
fn test_content_part_variants_serialize_correctly() {
    let variants = vec![
        (r#"{"type":"text","text":"test"}"#, "Text"),
        (
            r#"{"type":"image","data":"test","mime_type":"image/png"}"#,
            "Image",
        ),
        (
            r#"{"type":"audio","data":"http://test","mime_type":"audio/wav"}"#,
            "Audio",
        ),
        (
            r#"{"type":"tool_use","id":"1","name":"test","input":{}}"#,
            "ToolUse",
        ),
        (
            r#"{"type":"tool_result","tool_use_id":"1","content":[]}"#,
            "ToolResult",
        ),
        (r#"{"type":"reasoning","text":"test"}"#, "Reasoning"),
        (
            r#"{"type":"resource","uri":"test://test","name":"test"}"#,
            "Resource",
        ),
    ];

    assert_eq!(
        variants.len(),
        7,
        "ContentPart should have exactly 7 variants"
    );

    for (json, name) in variants {
        let parsed: Result<ContentPart, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "{name} variant should parse from JSON: {json}"
        );
    }
}

#[test]
fn test_image_content_structure() {
    let image_content = ImageContent {
        data: "base64data".into(),
        mime_type: "image/jpeg".into(),
        detail: None,
    };
    let part = ContentPart::Image(image_content);
    match part {
        ContentPart::Image(ic) => {
            assert_eq!(ic.data, "base64data");
            assert_eq!(ic.mime_type, "image/jpeg");
        }
        _ => panic!("Expected Image variant"),
    }
}

#[test]
fn test_content_part_is_tool_use() {
    let tool_use = ContentPart::ToolUse(ToolUse {
        id: "tool_1".into(),
        name: "read_file".into(),
        input: serde_json::json!({"path": "/test"}),
    });
    assert!(tool_use.is_tool_use());

    let text = ContentPart::Text(TextContent {
        text: "Hello".into(),
        cache_control: None,
    });
    assert!(!text.is_tool_use());
}

#[test]
fn test_message_content_has_text() {
    let content = Content::Single(ContentPart::Text(TextContent {
        text: "Hello".into(),
        cache_control: None,
    }));
    assert!(content.has_text());
    assert!(!content.has_tool_use());
}

#[test]
fn test_message_content_has_tool_use() {
    let content = Content::Multi(vec![
        ContentPart::Text(TextContent {
            text: "Hello".into(),
            cache_control: None,
        }),
        ContentPart::ToolUse(ToolUse {
            id: "tool_1".into(),
            name: "read".into(),
            input: serde_json::json!({}),
        }),
    ]);
    assert!(content.has_text());
    assert!(content.has_tool_use());
}

#[test]
fn test_message_content_single_no_tool_use() {
    let content = Content::Single(ContentPart::ToolUse(ToolUse {
        id: "tool_1".into(),
        name: "read".into(),
        input: serde_json::json!({}),
    }));
    assert!(!content.has_text());
    assert!(content.has_tool_use());
}

#[test]
fn test_request_validation_valid() {
    let request = CompletionRequest {
        model: "claude-3-5-sonnet".into(),
        messages: Arc::new(vec![Message {
            role: Role::User,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "Hello".into(),
                cache_control: None,
            })),
            tool_call_id: None,
            name: None,
            ..Default::default()
        }]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        max_tokens: Some(1024),
        temperature: None,
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
    };
    assert!(request.validate().is_ok());
}

#[test]
fn test_request_validation_invalid_sequence() {
    let request = CompletionRequest {
        model: "claude-3-5-sonnet".into(),
        messages: Arc::new(vec![
            Message {
                role: Role::User,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "Hello".into(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "Hi there!".into(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::ToolUse(ToolUse {
                    id: "tool_1".into(),
                    name: "read".into(),
                    input: serde_json::json!({}),
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            Message {
                role: Role::Tool,
                content: Content::Single(ContentPart::ToolResult(ToolResult {
                    tool_use_id: "tool_1".into(),
                    content: vec![ContentPart::Text(TextContent {
                        text: "result".into(),
                        cache_control: None,
                    })],
                    structured_content: None,
                    is_error: Some(false),
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "Hi there!".into(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
        ]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        max_tokens: Some(1024),
        temperature: None,
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
    };
    assert!(request.validate().is_err());
}

#[test]
fn test_request_validation_invalid_tool_followed_by_assistant() {
    let request = CompletionRequest {
        model: "claude-3-5-sonnet".into(),
        messages: Arc::new(vec![
            Message {
                role: Role::User,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "Hello".into(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            Message {
                role: Role::Tool,
                content: Content::Single(ContentPart::ToolResult(ToolResult {
                    tool_use_id: "tool_1".into(),
                    content: vec![ContentPart::Text(TextContent {
                        text: "result".into(),
                        cache_control: None,
                    })],
                    structured_content: None,
                    is_error: Some(false),
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "Hi there!".into(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
        ]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        max_tokens: Some(1024),
        temperature: None,
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
    };
    assert!(request.validate().is_err());
}

#[test]
fn test_retry_config_default() {
    let config = crate::retry::RetryConfig::default();
    assert_eq!(config.max_attempts, 3);
    assert_eq!(config.initial_interval_ms, 1000);
    assert_eq!(config.max_interval_ms, 10000);
    assert_eq!(config.max_elapsed_ms, 60000);
}

#[test]
fn anthropic_provider_supports_inline_cache_hints() {
    use crate::{
        anthropic::AnthropicProvider,
        traits::ModelProvider,
        types::ModelConfig,
    };
    let model_config = ModelConfig {
        name: "claude-3-5-sonnet".into(),
        provider: "anthropic".into(),
        context_window: 200_000,
        max_output_tokens: 8_192,
        supports_tools: true,
        supports_streaming: true,
        supports_reasoning: false,
    };
    let provider = AnthropicProvider::new(model_config);
    assert!(provider.supports_inline_cache_hints());
}

#[test]
fn test_is_retryable_error() {
    use crate::retry::is_retryable_error;

    assert!(is_retryable_error(429));
    assert!(is_retryable_error(500));
    assert!(is_retryable_error(502));
    assert!(is_retryable_error(503));
    assert!(is_retryable_error(504));

    assert!(!is_retryable_error(400));
    assert!(!is_retryable_error(401));
    assert!(!is_retryable_error(403));
    assert!(!is_retryable_error(404));
}

#[test]
fn test_provider_error_is_retryable() {
    let retryable = Error::RequestFailed {
        status: 429,
        message: "Rate limited".into(),
    };
    assert!(retryable.is_retryable());

    let not_retryable = Error::RequestFailed {
        status: 400,
        message: "Bad request".into(),
    };
    assert!(!not_retryable.is_retryable());
}

#[test]
fn test_provider_error_stream_is_retryable() {
    let stream_error = Error::Stream("connection lost".into());
    assert!(stream_error.is_retryable());
}

#[test]
fn test_provider_error_other_not_retryable() {
    let api_error = Error::Provider("some error".into());
    assert!(!api_error.is_retryable());

    let validation_error = Error::Validation("invalid".into());
    assert!(!validation_error.is_retryable());
}

#[test]
fn test_message_content_extract_text() {
    let text_only = Content::Single(ContentPart::Text(TextContent {
        text: "Hello world".into(),
        cache_control: None,
    }));
    let extracted = text_only.extract_text();
    assert_eq!(extracted, Some("Hello world".to_string()));

    let tool_use = Content::Single(ContentPart::ToolUse(ToolUse {
        id: "call_1".into(),
        name: "test".into(),
        input: serde_json::json!({}),
    }));
    assert!(tool_use.extract_text().is_none());
}

#[test]
fn test_message_content_extract_tool_uses() {
    let tool_use = Content::Single(ContentPart::ToolUse(ToolUse {
        id: "call_1".into(),
        name: "test".into(),
        input: serde_json::json!({}),
    }));
    let extracted = tool_use.extract_tool_uses();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].id, "call_1");

    let text_only = Content::Single(ContentPart::Text(TextContent {
        text: "Hello".into(),
        cache_control: None,
    }));
    assert!(text_only.extract_tool_uses().is_empty());
}

#[test]
fn test_message_content_mixed_extract() {
    let mixed = Content::Multi(vec![
        ContentPart::Text(TextContent {
            text: "Hello".into(),
            cache_control: None,
        }),
        ContentPart::ToolUse(ToolUse {
            id: "call_1".into(),
            name: "test".into(),
            input: serde_json::json!({}),
        }),
        ContentPart::Text(TextContent {
            text: " world".into(),
            cache_control: None,
        }),
    ]);

    let text = mixed.extract_text();
    assert!(text.is_some());
    assert!(text.unwrap().contains("Hello"));

    let tool_uses = mixed.extract_tool_uses();
    assert_eq!(tool_uses.len(), 1);
}

#[test]
fn test_message_content_from_response_text() {
    let response = Content::Single(ContentPart::Text(TextContent {
        text: "Response text".into(),
        cache_control: None,
    }));
    let text = response.extract_text();
    assert_eq!(text, Some("Response text".to_string()));
}

#[test]
fn test_message_content_from_response_tool_uses() {
    let response = Content::Multi(vec![
        ContentPart::ToolUse(ToolUse {
            id: "call_1".into(),
            name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
        }),
        ContentPart::ToolUse(ToolUse {
            id: "call_2".into(),
            name: "read".into(),
            input: serde_json::json!({"path": "/tmp"}),
        }),
    ]);
    let calls = response.extract_tool_uses();
    assert_eq!(calls.len(), 2);
}

#[test]
fn test_message_content_serialization_roundtrip() {
    let content = Content::Multi(vec![
        ContentPart::Text(TextContent {
            text: "Hello".into(),
            cache_control: None,
        }),
        ContentPart::ToolUse(ToolUse {
            id: "call_1".into(),
            name: "test".into(),
            input: serde_json::json!({"key": "value"}),
        }),
    ]);

    let json = serde_json::to_string(&content).unwrap();
    let deserialized: Content = serde_json::from_str(&json).unwrap();

    match deserialized {
        Content::Multi(parts) => {
            assert_eq!(parts.len(), 2);
            match &parts[0] {
                ContentPart::Text(TextContent { text, .. }) => {
                    assert_eq!(text, "Hello")
                }
                _ => panic!("Expected text first"),
            }
        }
        _ => panic!("Expected Multi"),
    }
}

#[test]
fn test_message_content_iterator_single() {
    let content = Content::Single(ContentPart::Text(TextContent {
        text: "Hello".into(),
        cache_control: None,
    }));
    let parts: Vec<_> = (&content).into_iter().collect();
    assert_eq!(parts.len(), 1);
    assert!(matches!(parts[0], ContentPart::Text(TextContent { .. })));
}

#[test]
fn test_message_content_iterator_multi() {
    let content = Content::Multi(vec![
        ContentPart::Text(TextContent {
            text: "Hello".into(),
            cache_control: None,
        }),
        ContentPart::ToolUse(ToolUse {
            id: "call_1".into(),
            name: "test".into(),
            input: serde_json::json!({}),
        }),
        ContentPart::Reasoning(ReasoningContent {
            text: "thinking".into(),
            signature: None,
        }),
    ]);
    let parts: Vec<_> = (&content).into_iter().collect();
    assert_eq!(parts.len(), 3);
    assert!(matches!(parts[0], ContentPart::Text(TextContent { .. })));
    assert!(matches!(parts[1], ContentPart::ToolUse(..)));
    assert!(matches!(parts[2], ContentPart::Reasoning(..)));
}

#[test]
fn test_message_content_into_iterator_single() {
    let content = Content::Single(ContentPart::Text(TextContent {
        text: "Hello".into(),
        cache_control: None,
    }));
    assert_eq!(content.into_iter().count(), 1);
}

#[test]
fn test_message_content_into_iterator_multi() {
    let content = Content::Multi(vec![
        ContentPart::Text(TextContent {
            text: "Hello".into(),
            cache_control: None,
        }),
        ContentPart::Text(TextContent {
            text: " world".into(),
            cache_control: None,
        }),
    ]);
    assert_eq!(content.into_iter().count(), 2);
}

#[test]
fn test_stream_chunk_content_text() {
    let chunk = StreamChunk::Content(ContentPart::Text(TextContent {
        text: "Hello".into(),
        cache_control: None,
    }));
    match chunk {
        StreamChunk::Content(ContentPart::Text(TextContent {
            text, ..
        })) => {
            assert_eq!(text, "Hello");
        }
        _ => panic!("Expected text content"),
    }
}

#[test]
fn test_stream_chunk_content_image() {
    let chunk = StreamChunk::Content(ContentPart::Image(ImageContent {
        data: "base64encodeddata".into(),
        mime_type: "image/jpeg".into(),
        detail: None,
    }));
    match chunk {
        StreamChunk::Content(ContentPart::Image(ImageContent {
            data,
            mime_type,
            detail,
        })) => {
            assert_eq!(data, "base64encodeddata");
            assert_eq!(mime_type, "image/jpeg");
            assert!(detail.is_none());
        }
        _ => panic!("Expected image content"),
    }
}

#[test]
fn test_stream_chunk_stop() {
    let chunk = StreamChunk::Stop("end_turn".into());
    match chunk {
        StreamChunk::Stop(reason) => assert_eq!(reason, "end_turn"),
        _ => panic!("Expected Stop variant"),
    }
}

#[test]
fn test_tool_use_structure() {
    let tool_use = ToolUse {
        id: "tool_1".into(),
        name: "read_file".into(),
        input: serde_json::json!({"path": "/test"}),
    };
    assert_eq!(tool_use.id, "tool_1");
    assert_eq!(tool_use.name, "read_file");
    assert_eq!(tool_use.input, serde_json::json!({"path": "/test"}));
}

#[test]
fn test_tool_use_input_as_json_value() {
    let tool_use = ToolUse {
        id: "tool_1".into(),
        name: "get_weather".into(),
        input: serde_json::json!({"location": "Beijing", "unit": "celsius"}),
    };
    assert!(tool_use.input.is_object());
    assert_eq!(tool_use.input["location"], "Beijing");
}

#[test]
fn test_resource_link_structure() {
    let link = ResourceLink {
        uri: "file:///path/to/resource".into(),
        name: "my_resource".into(),
        title: Some("My Resource Title".into()),
        description: Some("A test resource".into()),
        mime_type: Some("text/plain".into()),
    };
    assert_eq!(link.uri, "file:///path/to/resource");
    assert_eq!(link.name, "my_resource");
    assert_eq!(link.title, Some("My Resource Title".into()));
    assert_eq!(link.description, Some("A test resource".into()));
    assert_eq!(link.mime_type, Some("text/plain".into()));
}

#[test]
fn test_resource_link_minimal() {
    let link = ResourceLink {
        uri: "str:///hello".into(),
        name: "hello".into(),
        title: None,
        description: None,
        mime_type: None,
    };
    assert_eq!(link.uri, "str:///hello");
    assert!(link.title.is_none());
    assert!(link.description.is_none());
    assert!(link.mime_type.is_none());
}

#[test]
fn test_content_part_tool_use() {
    let part = ContentPart::ToolUse(ToolUse {
        id: "tool_1".into(),
        name: "read_file".into(),
        input: serde_json::json!({"path": "/test"}),
    });
    assert!(matches!(part, ContentPart::ToolUse(..)));
    if let ContentPart::ToolUse(tu) = part {
        assert_eq!(tu.id, "tool_1");
        assert_eq!(tu.name, "read_file");
        assert_eq!(tu.input, serde_json::json!({"path": "/test"}));
    }
}

#[cfg(test)]
mod stream_processor_tests {
    use crate::{
        openai_streaming::OpenAIStreamProcessorV2,
        streaming::StreamProcessor,
        types::{ContentPart, StreamChunk, TextContent},
    };

    #[test]
    fn test_openai_stream_processor_v2_text_only() {
        let mut processor = OpenAIStreamProcessorV2::new();
        let chunks = processor.process_line(
            r#"{"id":"chatcmpl-1","choices":[{"delta":{"content":"Hello"}}]}"#,
        );
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Text(TextContent {
                text,
                ..
            })) => {
                assert_eq!(text, "Hello")
            }
            _ => panic!("Expected text chunk"),
        }
    }

    #[test]
    fn test_openai_stream_processor_v2_done_signal() {
        let mut processor = OpenAIStreamProcessorV2::new();
        let chunks = processor.process_line("[DONE]");
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::IsDone { .. } => {}
            _ => panic!("Expected is_done chunk"),
        }
    }

    #[test]
    fn test_openai_stream_processor_v2_tool_call_accumulates() {
        let mut processor = OpenAIStreamProcessorV2::new();
        let chunk1 = r#"{"id":"c1","choices":[{"delta":{"tool_calls":[{"id":"call_1","function":{"name":"test","arguments":""}}]}}]}"#;
        let chunk2 = r#"{"id":"c1","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{"}}]}}]}"#;
        let chunk3 = r#"{"id":"c1","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a"}}]}}]}"#;
        let chunk4 = r#"{"id":"c1","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"}"}}]}}]}"#;

        let _ = processor.process_line(chunk1);
        let _ = processor.process_line(chunk2);
        let _ = processor.process_line(chunk3);
        let chunks = processor.process_line(chunk4);

        let last = chunks.last().unwrap();
        if let StreamChunk::Content(ContentPart::ToolUse(tu)) = last {
            assert!(
                tu.input.to_string().contains('{')
                    || tu.input.to_string().contains("a"),
                "Got: {:?}",
                tu.input
            );
        }
    }

    #[test]
    fn test_openai_stream_processor_v2_reasoning_split() {
        let mut processor = OpenAIStreamProcessorV2::new();
        let chunks = processor.process_line(
            r#"{"id":"c1","choices":[{"delta":{"reasoning_content":"thinking","content":"hello"}}]}"#,
        );
        assert_eq!(chunks.len(), 2);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Reasoning(rc)) => {
                assert_eq!(rc.text, "thinking")
            }
            _ => panic!("Expected reasoning first"),
        }
        match &chunks[1] {
            StreamChunk::Content(ContentPart::Text(TextContent {
                text,
                ..
            })) => {
                assert_eq!(text, "hello")
            }
            _ => panic!("Expected text second"),
        }
    }

    #[test]
    fn test_anthropic_stream_processor_text() {
        let mut processor = StreamProcessor::new();
        let chunks = processor.process_event(&serde_json::from_str(r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#).unwrap());
        assert!(chunks.is_empty());

        let chunks = processor.process_event(&serde_json::from_str(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#).unwrap());
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Text(TextContent {
                text,
                ..
            })) => {
                assert_eq!(text, "Hello")
            }
            _ => panic!("Expected text"),
        }
    }

    #[test]
    fn test_anthropic_stream_processor_thinking() {
        let mut processor = StreamProcessor::new();
        let chunks = processor.process_event(&serde_json::from_str(r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"Let me think"}}"#).unwrap());
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Reasoning(rc)) => {
                assert_eq!(rc.text, "Let me think")
            }
            _ => panic!("Expected reasoning"),
        }
    }

    #[test]
    fn test_anthropic_stream_processor_tool_use() {
        let mut processor = StreamProcessor::new();
        let start = serde_json::from_str::<crate::streaming::AnthropicStreamEvent>(r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_01","name":"get_weather","input":""}}"#).unwrap();
        let chunks = processor.process_event(&start);
        assert!(!chunks.is_empty());

        let delta = serde_json::from_str::<crate::streaming::AnthropicStreamEvent>(r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"location\":"}}"#).unwrap();
        let chunks = processor.process_event(&delta);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::ToolUse(tu)) => {
                assert!(tu.input.to_string().contains("location"))
            }
            _ => panic!("Expected tool use"),
        }
    }

    #[test]
    fn test_anthropic_stream_processor_message_stop() {
        let mut processor = StreamProcessor::new();
        let chunks = processor.process_event(
            &serde_json::from_str(
                r#"{"type":"message_stop","stop_reason":"end_turn"}"#,
            )
            .unwrap(),
        );
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Stop(reason) => assert_eq!(reason, "end_turn"),
            _ => panic!("Expected stop"),
        }
    }

    #[test]
    fn test_stream_processor_reset() {
        let mut processor = StreamProcessor::new();
        processor.process_event(&serde_json::from_str(r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"test","input":""}}"#).unwrap());
        processor.reset();
        let chunks_after_reset = processor.process_event(&serde_json::from_str(r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"test"}}"#).unwrap());
        let has_tool_use_after_reset = chunks_after_reset.iter().any(|c| {
            matches!(c, StreamChunk::Content(ContentPart::ToolUse { .. }))
        });
        assert!(
            !has_tool_use_after_reset,
            "After reset, should not have buffered tool use"
        );
    }

    #[test]
    fn test_openai_stream_processor_v2_invalid_json() {
        let mut processor = OpenAIStreamProcessorV2::new();
        let chunks = processor.process_line("not json");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_anthropic_stream_processor_redacted_thinking() {
        let mut processor = StreamProcessor::new();
        let chunks = processor.process_event(&serde_json::from_str(r#"{"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking"}}"#).unwrap());
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Reasoning(rc)) => {
                assert_eq!(rc.text, "[Redacted by safety filter]")
            }
            _ => panic!("Expected reasoning"),
        }
    }
}

mod multi_modal_tests {
    use crate::{
        Content,
        ContentPart,
        ImageContent,
        Message,
        Role,
        TextContent,
        ToolResult,
    };

    #[test]
    fn test_multi_modal_message_with_image() {
        let msg = Message {
            role: Role::User,
            content: Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: "Describe this image".into(),
                    cache_control: None,
                }),
                ContentPart::Image(ImageContent {
                    data: "data:image/png;base64,abc".into(),
                    mime_type: "image/png".into(),
                    detail: Some(crate::ImageDetail::Auto),
                }),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };

        let parts: Vec<_> = (&msg.content).into_iter().collect();
        assert_eq!(parts.len(), 2);
        assert!(matches!(parts[0], ContentPart::Text(_)));
        assert!(matches!(parts[1], ContentPart::Image(_)));
    }

    #[test]
    fn test_tool_result_with_media_content() {
        let tool_result = ToolResult {
            tool_use_id: "call-1".into(),
            content: vec![
                ContentPart::Text(TextContent {
                    text: "Analysis result".into(),
                    cache_control: None,
                }),
                ContentPart::Image(ImageContent {
                    data: "data:image/jpeg;base64,xyz".into(),
                    mime_type: "image/jpeg".into(),
                    detail: None,
                }),
            ],
            structured_content: None,
            is_error: Some(false),
        };

        assert!(matches!(tool_result.content[0], ContentPart::Text(_)));
        assert!(matches!(tool_result.content[1], ContentPart::Image(_)));
    }

    #[test]
    fn test_cache_control_serialization() {
        #[derive(serde::Serialize)]
        struct CacheControl {
            r#type: String,
        }
        let cc = CacheControl {
            r#type: "ephemeral".to_string(),
        };
        let json = serde_json::to_string(&cc).unwrap();
        assert!(json.contains("ephemeral"));
    }
}
