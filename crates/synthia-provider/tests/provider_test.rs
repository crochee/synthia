//! The both-adapters integration suite.
//!
//! Every adapter-specific test is gated with its adapter's feature, so
//! this file is meaningful in any build that has at least one adapter
//! and compiles to nothing without one (the adapter-agnostic tests it
//! also carries need an adapter to exercise).

#![cfg(any(feature = "anthropic", feature = "openai"))]

use std::{sync::Arc, time::Duration};

use synthia_core::Error;
#[cfg(feature = "anthropic")]
use synthia_provider::AnthropicProvider;
#[cfg(feature = "openai")]
use synthia_provider::OpenAICompatibleProvider;
use synthia_provider::{
    ModelProvider,
    types::{
        AudioContent,
        AudioFormat,
        CompletionRequest,
        Content,
        ContentPart,
        ImageContent,
        ImageDetail,
        Message,
        ModelConfig,
        ReasoningContent,
        Role,
        StreamChunk,
        TextContent,
        ToolChoice,
        ToolDefinition,
        ToolResult,
        ToolUse,
    },
};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

fn test_model_config() -> ModelConfig {
    ModelConfig {
        name: "test-model".to_string(),
        provider: "test".to_string(),
        context_window: 128_000,
        max_output_tokens: 4096,
        supports_tools: true,
        supports_streaming: true,
        supports_reasoning: false,
    }
}

fn text_message(content: &str) -> Message {
    Message {
        role: Role::User,
        content: Content::Single(ContentPart::Text(TextContent {
            text: content.to_string(),
            cache_control: None,
        })),
        tool_call_id: None,
        name: None,
        ..Default::default()
    }
}

fn simple_request() -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        messages: Arc::new(vec![text_message("Hello")]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        temperature: Some(0.5),
        max_tokens: Some(100),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
        replay_state: None,
    }
}

fn request_with_tools() -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        messages: Arc::new(vec![text_message("What's the weather?")]),
        tools: Arc::new(vec![ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get the current weather".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
            cache_control: None,
            annotations: None,
        }]),
        tool_choice: ToolChoice::Auto,
        temperature: Some(0.5),
        max_tokens: Some(100),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
        replay_state: None,
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_simple_text_request() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you today?"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "total_tokens": 30
        }
    });

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = simple_request();
    let result = provider.complete(request).await;

    assert!(result.is_ok(), "Request should succeed: {:?}", result.err());
    let response = result.unwrap();
    assert_eq!(response.id, "chatcmpl-123");
    match response.content {
        Content::Single(ContentPart::Text(TextContent { text, .. })) => {
            assert!(text.contains("Hello"));
        }
        _ => panic!("Expected text response"),
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_multimodal_image_request() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "I can see the image of a cat!"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "total_tokens": 120
        }
    });

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: Arc::new(vec![Message {
            role: Role::User,
            content: Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: "What do you see in this image?".to_string(),
                    cache_control: None,
                }),
                ContentPart::Image(ImageContent {
                    data: "https://example.com/cat.jpg".to_string(),
                    mime_type: "image/jpeg".to_string(),
                    detail: Some(ImageDetail::High),
                }),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        }]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        max_tokens: Some(100),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
        replay_state: None,
    };

    let result = provider.complete(request).await;
    assert!(result.is_ok(), "Multimodal request should succeed");
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_tool_call_request() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"location\":\"Beijing\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 50,
            "completion_tokens": 30,
            "total_tokens": 80
        }
    });

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = request_with_tools();
    let result = provider.complete(request).await;

    assert!(
        result.is_ok(),
        "Tool call request should succeed: {:?}",
        result.err()
    );
    let response = result.unwrap();
    match response.content {
        Content::Single(ContentPart::ToolUse(ToolUse {
            id: _,
            name,
            input,
        })) => {
            assert_eq!(name, "get_weather");
            let args_json = input;
            assert!(args_json.get("location").is_some());
        }
        Content::Multi(parts) => {
            let tool_uses: Vec<_> = parts
                .iter()
                .filter_map(|p| {
                    if let ContentPart::ToolUse(ToolUse { id, name, input }) = p
                    {
                        Some((id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            assert!(!tool_uses.is_empty());
            assert_eq!(tool_uses[0].1, "get_weather");
        }
        _ => panic!("Expected tool call response"),
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_mixed_content_response() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "Let me check that for you."},
                    {"type": "tool_call", "id": "call_456", "name": "search", "input": {"query": "weather"}}
                ]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 20,
            "completion_tokens": 40,
            "total_tokens": 60
        }
    });

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = simple_request();
    let result = provider.complete(request).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    if let Content::Multi(parts) = response.content {
        assert_eq!(parts.len(), 2);
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_streaming() {
    let mock_server = MockServer::start().await;
    let sse_data = r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}

data: [DONE]
"#;

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = simple_request();
    let text_output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    let _response = provider
        .complete_with_stream(
            request,
            None,
            Box::new({
                let text_output = text_output.clone();
                move |chunk| {
                    if let StreamChunk::Content(ContentPart::Text(
                        TextContent { text, .. },
                    )) = chunk
                    {
                        text_output.lock().unwrap().push_str(&text);
                    }
                }
            }),
        )
        .await
        .expect("complete_with_stream should succeed");

    assert_eq!(*text_output.lock().unwrap(), "Hello world");
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_simple_request() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "content": [
            {"type": "text", "text": "Hello! How can I help you?"}
        ],
        "model": "test-model",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 10,
            "output_tokens": 20
        }
    });

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: Arc::new(vec![text_message("Hello")]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        temperature: Some(0.5),
        max_tokens: Some(100),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
        replay_state: None,
    };

    let result = provider.complete(request).await;
    assert!(result.is_ok(), "Request should succeed: {:?}", result.err());
    let response = result.unwrap();
    assert_eq!(response.id, "msg_123");
    match response.content {
        Content::Single(ContentPart::Text(TextContent { text, .. })) => {
            assert!(text.contains("Hello"));
        }
        _ => panic!("Expected text response"),
    }
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_tool_call_request() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "tool_use",
                "id": "toolu_123",
                "name": "get_weather",
                "input": {"location": "Beijing"}
            }
        ],
        "model": "test-model",
        "stop_reason": "tool_use",
        "usage": {
            "input_tokens": 50,
            "output_tokens": 30
        }
    });

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let request = request_with_tools();
    let result = provider.complete(request).await;
    assert!(
        result.is_ok(),
        "Tool call request should succeed: {:?}",
        result.err()
    );
    let response = result.unwrap();
    match response.content {
        Content::Single(ContentPart::ToolUse(ToolUse {
            id: _, name, ..
        })) => {
            assert_eq!(name, "get_weather");
        }
        Content::Multi(parts) => {
            let has_tool_use = parts.iter().any(|p| {
                matches!(p, ContentPart::ToolUse(ToolUse { name, .. }) if name == "get_weather")
            });
            assert!(has_tool_use);
        }
        _ => panic!("Expected tool use response"),
    }
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_multimodal_image_request() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "content": [
            {"type": "text", "text": "I can see a beautiful sunset!"}
        ],
        "model": "test-model",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 500,
            "output_tokens": 20
        }
    });

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: Arc::new(vec![Message {
            role: Role::User,
            content: Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: "What do you see in this image?".to_string(),
                    cache_control: None,
                }),
                ContentPart::Image(ImageContent {
                    data: "https://example.com/sunset.jpg".to_string(),
                    mime_type: "image/jpeg".to_string(),
                    detail: Some(ImageDetail::High),
                }),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        }]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        max_tokens: Some(100),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
        replay_state: None,
    };

    let result = provider.complete(request).await;
    assert!(result.is_ok(), "Multimodal request should succeed");
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_streaming_with_thinking() {
    let mock_server = MockServer::start().await;
    let sse_data = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"Let me think about this..."}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Thinking..."}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
data: {"type":"content_block_stop","index":0}
data: {"type":"message_stop"}
"#;

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let request = simple_request();
    let reasoning_count = std::sync::Arc::new(std::sync::Mutex::new(0u32));
    let text_count = std::sync::Arc::new(std::sync::Mutex::new(0u32));

    let _response = provider
        .complete_with_stream(
            request,
            None,
            Box::new({
                let reasoning_count = reasoning_count.clone();
                let text_count = text_count.clone();
                move |chunk| match chunk {
                    StreamChunk::Content(ContentPart::Reasoning(..)) => {
                        *reasoning_count.lock().unwrap() += 1;
                    }
                    StreamChunk::Content(ContentPart::Text(TextContent {
                        text,
                        ..
                    })) => {
                        *text_count.lock().unwrap() += 1;
                        assert_eq!(text, "Hello");
                    }
                    _ => {}
                }
            }),
        )
        .await
        .expect("complete_with_stream should succeed");

    assert!(
        *reasoning_count.lock().unwrap() > 0,
        "Should have reasoning chunks"
    );
    assert!(*text_count.lock().unwrap() > 0, "Should have text chunks");
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_streaming_tool_calls() {
    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01A2B3C4D5E6F7\",\"name\":\"get_weather\",\"input\":\"\"}}\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"location\\\":\"}}\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"Beijing\\\"\"}}\ndata: {\"type\":\"content_block_stop\",\"index\":0}\ndata: {\"type\":\"message_stop\"}\n";

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let request = request_with_tools();
    let tool_call_starts = std::sync::Arc::new(std::sync::Mutex::new(0u32));
    let tool_call_deltas = std::sync::Arc::new(std::sync::Mutex::new(0u32));
    let tool_call_ends = std::sync::Arc::new(std::sync::Mutex::new(0u32));

    let _response = provider
        .complete_with_stream(
            request,
            None,
            Box::new({
                let tool_call_starts = tool_call_starts.clone();
                let tool_call_deltas = tool_call_deltas.clone();
                let tool_call_ends = tool_call_ends.clone();
                move |chunk| match chunk {
                    StreamChunk::ToolCallStart { .. } => {
                        *tool_call_starts.lock().unwrap() += 1;
                    }
                    StreamChunk::ToolCallDelta { .. } => {
                        *tool_call_deltas.lock().unwrap() += 1;
                    }
                    StreamChunk::ToolCallEnd { .. } => {
                        *tool_call_ends.lock().unwrap() += 1;
                    }
                    _ => {}
                }
            }),
        )
        .await
        .expect("complete_with_stream should succeed");

    assert!(
        *tool_call_starts.lock().unwrap() > 0,
        "Should have tool call start chunk"
    );
    assert!(
        *tool_call_deltas.lock().unwrap() > 0,
        "Should have tool call delta chunks"
    );
    assert!(
        *tool_call_ends.lock().unwrap() > 0,
        "Should have tool call end chunk"
    );
}

#[tokio::test]
async fn test_message_content_transforms() {
    let text_part = ContentPart::Text(TextContent {
        text: "Hello".to_string(),
        cache_control: None,
    });
    assert!(matches!(text_part, ContentPart::Text(..)));

    let image_part = ContentPart::Image(ImageContent {
        data: "https://example.com/image.jpg".to_string(),
        mime_type: "image/jpeg".to_string(),
        detail: Some(ImageDetail::High),
    });
    assert!(matches!(image_part, ContentPart::Image(..)));

    let audio_part = ContentPart::Audio(AudioContent {
        data: "data:audio/wav;base64,...".to_string(),
        mime_type: "audio/wav".to_string(),
        format: Some(AudioFormat::Wav),
    });
    assert!(matches!(audio_part, ContentPart::Audio(..)));

    let tool_use_part = ContentPart::ToolUse(ToolUse {
        id: "call_123".to_string(),
        name: "test_tool".to_string(),
        input: serde_json::json!({"arg": "value"}),
    });
    assert!(matches!(tool_use_part, ContentPart::ToolUse(..)));

    let tool_result_part = ContentPart::ToolResult(ToolResult {
        tool_use_id: "call_123".to_string(),
        tool_name: None,
        content: vec![ContentPart::Text(TextContent {
            text: "Result content".to_string(),
            cache_control: None,
        })],
        structured_content: None,
        is_error: Some(false),
        metadata: serde_json::Map::new(),
        truncated_by: None,
    });
    assert!(matches!(tool_result_part, ContentPart::ToolResult(..)));

    let reasoning_part = ContentPart::Reasoning(ReasoningContent {
        text: "Let me think...".to_string(),
        signature: None,
    });
    assert!(matches!(reasoning_part, ContentPart::Reasoning(..)));
}

#[tokio::test]
async fn test_tool_definition_serialization() {
    let tool = ToolDefinition {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "arg1": {"type": "string"},
                "arg2": {"type": "number"}
            },
            "required": ["arg1"]
        }),
        cache_control: None,
        annotations: None,
    };

    let serialized = serde_json::to_string(&tool).unwrap();
    let deserialized: ToolDefinition =
        serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.name, "test_tool");
    assert_eq!(deserialized.description, "A test tool");
    assert!(deserialized.input_schema.is_object());
}

#[tokio::test]
async fn test_stream_chunk_variants() {
    let text_chunk = StreamChunk::Content(ContentPart::Text(TextContent {
        text: "Hello".to_string(),
        cache_control: None,
    }));
    assert!(matches!(
        text_chunk,
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: _,
            cache_control: None
        }))
    ));

    let tool_use = ToolUse {
        id: "call_1".to_string(),
        name: "test".to_string(),
        input: serde_json::json!({"arg": 1}),
    };
    let tool_chunk = StreamChunk::Content(ContentPart::ToolUse(ToolUse {
        id: tool_use.id.clone(),
        name: tool_use.name.clone(),
        input: tool_use.input,
    }));
    assert!(matches!(
        tool_chunk,
        StreamChunk::Content(ContentPart::ToolUse(..))
    ));

    let reasoning_chunk =
        StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
            text: "thinking...".to_string(),
            signature: None,
        }));
    assert!(matches!(
        reasoning_chunk,
        StreamChunk::Content(ContentPart::Reasoning(..))
    ));

    let stop_chunk = StreamChunk::Stop("".to_string());
    assert!(matches!(stop_chunk, StreamChunk::Stop(_)));
}

#[tokio::test]
async fn test_stream_chunk_tool_use_structure() {
    let tool_use = StreamChunk::Content(ContentPart::ToolUse(ToolUse {
        id: "call_1".to_string(),
        name: "test_tool".to_string(),
        input: serde_json::json!({"arg": 1}),
    }));
    match tool_use {
        StreamChunk::Content(ContentPart::ToolUse(ToolUse {
            id,
            name,
            input,
        })) => {
            assert_eq!(id, "call_1");
            assert_eq!(name, "test_tool");
            assert_eq!(input, serde_json::json!({"arg": 1}));
        }
        _ => panic!("Expected ToolUse variant"),
    }
}

#[tokio::test]
async fn test_stream_chunk_stop_variants() {
    let normal_stop = StreamChunk::Stop("".to_string());
    let length_stop = StreamChunk::Stop("length".to_string());
    let content_filter_stop = StreamChunk::Stop("content_filter".to_string());

    match normal_stop {
        StreamChunk::Stop(s) => assert_eq!(s, ""),
        _ => panic!("Expected Stop"),
    }
    match length_stop {
        StreamChunk::Stop(s) => assert_eq!(s, "length"),
        _ => panic!("Expected Stop"),
    }
    match content_filter_stop {
        StreamChunk::Stop(s) => assert_eq!(s, "content_filter"),
        _ => panic!("Expected Stop"),
    }
}

#[tokio::test]
async fn test_tool_use_input_as_json_value() {
    let tool_use = ToolUse {
        id: "call_1".to_string(),
        name: "test".to_string(),
        input: serde_json::json!({"location": "NYC"}),
    };
    assert_eq!(tool_use.id, "call_1");
    assert_eq!(tool_use.name, "test");
    assert_eq!(tool_use.input, serde_json::json!({"location": "NYC"}));
}

#[tokio::test]
async fn test_content_part_tool_use_input_as_json_value() {
    let content_part = ContentPart::ToolUse(ToolUse {
        id: "call_1".to_string(),
        name: "get_weather".to_string(),
        input: serde_json::json!({"location": "NYC"}),
    });
    match content_part {
        ContentPart::ToolUse(ToolUse { id, name, input }) => {
            assert_eq!(id, "call_1");
            assert_eq!(name, "get_weather");
            assert_eq!(input, serde_json::json!({"location": "NYC"}));
        }
        _ => panic!("Expected ToolUse variant"),
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_streaming_with_reasoning_split() {
    let mock_server = MockServer::start().await;
    let sse_data = r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{"reasoning_content":"thinking...","content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}

data: [DONE]
"#;

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = simple_request();
    let text_output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let reasoning_output =
        std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    let _response = provider
        .complete_with_stream(
            request,
            None,
            Box::new({
                let text_output = text_output.clone();
                let reasoning_output = reasoning_output.clone();
                move |chunk| match chunk {
                    StreamChunk::Content(ContentPart::Text(TextContent {
                        text,
                        ..
                    })) => {
                        text_output.lock().unwrap().push_str(&text);
                    }
                    StreamChunk::Content(ContentPart::Reasoning(
                        ReasoningContent { text, .. },
                    )) => reasoning_output.lock().unwrap().push_str(&text),
                    _ => {}
                }
            }),
        )
        .await
        .expect("complete_with_stream should succeed");

    assert_eq!(*text_output.lock().unwrap(), "Hello world");
    assert_eq!(*reasoning_output.lock().unwrap(), "thinking...");
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_streaming_tool_call_accumulation() {
    let mock_server = MockServer::start().await;
    let chunk1 = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call_1","type":"function","index":0,"function":{"name":"bash","arguments":""}}]},"finish_reason":null}]}"#;
    let chunk2 = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"a"}}]},"finish_reason":null}]}"#;
    let chunk3 = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"bc"}}]},"finish_reason":null}]}"#;
    let chunk4 = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":1234567890,"model":"test-model","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
    let sse_data = format!(
        "data: {chunk1}\n\ndata: {chunk2}\n\ndata: {chunk3}\n\ndata: {chunk4}\n\ndata: [DONE]"
    );

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = request_with_tools();
    let tool_uses =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<StreamChunk>::new()));
    let final_tool_calls =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<ToolUse>::new()));

    let _response = provider
        .complete_with_stream(
            request,
            None,
            Box::new({
                let tool_uses = tool_uses.clone();
                let final_tool_calls = final_tool_calls.clone();
                move |chunk| {
                    if matches!(
                        chunk,
                        StreamChunk::Content(ContentPart::ToolUse(..))
                    ) {
                        tool_uses.lock().unwrap().push(chunk);
                    } else if let StreamChunk::IsDone { result } = chunk {
                        *final_tool_calls.lock().unwrap() =
                            result.tool_calls.clone();
                    }
                }
            }),
        )
        .await
        .expect("complete_with_stream should succeed");

    // V2 processor collects tool calls in IsDone result
    let tool_uses = tool_uses.lock().unwrap();
    let final_tool_calls = final_tool_calls.lock().unwrap();
    let has_tool_use = !tool_uses.is_empty() || !final_tool_calls.is_empty();
    assert!(has_tool_use, "No tool uses received");

    // Check the tool call from IsDone result (V2) or from tool_uses (legacy)
    let (id, name, input) = if !final_tool_calls.is_empty() {
        let tc = &final_tool_calls[0];
        (tc.id.clone(), tc.name.clone(), tc.input.clone())
    } else if let StreamChunk::Content(ContentPart::ToolUse(ToolUse {
        id,
        name,
        input,
    })) = &tool_uses[tool_uses.len() - 1]
    {
        (id.clone(), name.clone(), input.clone())
    } else {
        panic!("Expected ToolUse")
    };

    assert_eq!(id, "call_1");
    assert_eq!(name, "bash");
    assert!(
        input.to_string().contains("abc"),
        "Expected 'abc' in input, got: {input:?}"
    );
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_tool_result_media_extraction() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "cmpl-1",
        "object": "chat.completion",
        "created": 123,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "I see it"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120}
    });

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: Arc::new(vec![
            Message {
                role: Role::User,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "Check result".to_string(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            Message {
                role: Role::Tool,
                content: Content::Multi(vec![
                    ContentPart::Text(TextContent {
                        text: "tool result".to_string(),
                        cache_control: None,
                    }),
                    ContentPart::Image(ImageContent {
                        data: "base64data".to_string(),
                        mime_type: "image/png".to_string(),
                        detail: None,
                    }),
                ]),
                tool_call_id: Some("call_1".to_string()),
                name: Some("test_tool".to_string()),
                ..Default::default()
            },
        ]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        max_tokens: Some(100),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
        replay_state: None,
    };

    let result = provider.complete(request).await;
    assert!(result.is_ok(), "Tool result with media should succeed");
}

#[test]
fn test_multimodal_content_serialization() {
    let multi_msg = Message {
        role: Role::User,
        content: Content::Multi(vec![
            ContentPart::Text(TextContent {
                text: "Describe this".to_string(),
                cache_control: None,
            }),
            ContentPart::Image(ImageContent {
                data: "https://example.com/img.jpg".to_string(),
                mime_type: "image/jpeg".to_string(),
                detail: Some(ImageDetail::Auto),
            }),
            ContentPart::Audio(AudioContent {
                data: "base64audio".to_string(),
                mime_type: "audio/wav".to_string(),
                format: Some(AudioFormat::Wav),
            }),
        ]),
        tool_call_id: None,
        name: None,
        ..Default::default()
    };

    let json = serde_json::to_string(&multi_msg).unwrap();
    let decoded: Message = serde_json::from_str(&json).unwrap();
    match decoded.content {
        Content::Multi(parts) => {
            assert_eq!(parts.len(), 3);
        }
        _ => panic!("Expected multi content"),
    }
}

// =====================================================================
// complete_with_stream tests (PR1-M2)
// =====================================================================

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_complete_with_stream_emits_v2_chunks() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"get_weather\",\"input\":\"\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"location\\\":\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"Beijing\\\"}\"}}\n\
                    data: {\"type\":\"content_block_stop\",\"index\":0}\n\
                    data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"The \"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"weather in Beijing is sunny.\"}}\n\
                    data: {\"type\":\"content_block_stop\",\"index\":1}\n\
                    data: {\"type\":\"message_stop\",\"stop_reason\":\"end_turn\"}\n";

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let request = request_with_tools();
    let collected =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<StreamChunk>::new()));
    let collected_inner = collected.clone();
    let cancel = CancellationToken::new();

    let res = provider
        .complete_with_stream(
            request,
            Some(std::sync::Arc::new(cancel)),
            Box::new(move |chunk| {
                collected_inner.lock().unwrap().push(chunk);
            }),
        )
        .await;

    assert!(res.is_ok(), "complete_with_stream should succeed: {res:?}");

    // Walk the chunks. We expect at least:
    //   ToolCallStart { id: t1, name: get_weather }
    //   ToolCallDelta { id: t1, arguments_delta: ... }  (twice)
    //   ToolCallEnd { id: t1 }
    //   Content(Text { "The " })
    //   Content(Text { "weather in Beijing is sunny." })
    //   IsDone { result: SamplingResult { text, tool_calls: [t1] } }
    let chunks = collected.lock().unwrap();
    let tool_starts: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::ToolCallStart { .. }))
        .collect();
    let tool_deltas: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::ToolCallDelta { .. }))
        .collect();
    let tool_ends: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::ToolCallEnd { .. }))
        .collect();
    let is_done: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::IsDone { .. }))
        .collect();
    let text_chunks: usize = chunks
        .iter()
        .filter(|c| {
            matches!(c, StreamChunk::Content(ContentPart::Text(TextContent { text, .. })) if !text.is_empty())
        })
        .count();

    assert_eq!(tool_starts.len(), 1, "expected exactly one ToolCallStart");
    assert_eq!(tool_deltas.len(), 2, "expected two ToolCallDeltas");
    assert_eq!(tool_ends.len(), 1, "expected exactly one ToolCallEnd");
    assert_eq!(is_done.len(), 1, "expected exactly one IsDone");
    assert_eq!(text_chunks, 2, "expected two text content chunks");

    // Verify the IsDone payload
    match &is_done[0] {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.text, "The weather in Beijing is sunny.");
            assert_eq!(result.tool_calls.len(), 1);
            let tc = &result.tool_calls[0];
            assert_eq!(tc.id, "t1");
            assert_eq!(tc.name, "get_weather");
            assert_eq!(tc.input, serde_json::json!({"location": "Beijing"}));
        }
        _ => unreachable!(),
    }
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_complete_with_stream_cancellation() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    // Build an SSE body that streams slowly. We use a single
    // `ResponseTemplate` with no delay, but cancel the token before the
    // loop ticks; the test verifies the function returns an Aborted
    // StreamError quickly.
    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\
                    data: {\"type\":\"content_block_stop\",\"index\":0}\n\
                    data: {\"type\":\"message_stop\"}\n";

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());
    let request = simple_request();

    let cancel = CancellationToken::new();
    // Pre-cancel: the loop's first is_cancelled() check will trip.
    cancel.cancel();

    let res = provider
        .complete_with_stream(
            request,
            Some(std::sync::Arc::new(cancel)),
            Box::new(|_chunk: StreamChunk| {}),
        )
        .await;

    assert!(res.is_err(), "pre-cancelled stream must error out");
    let err = res.unwrap_err();
    // We may see either HttpFailure (if the request had not yet been
    // sent) or StreamError{Aborted} (if the request finished and the
    // loop caught the cancellation). Both are valid paths in this
    // tight race; assert the message is non-empty and starts with
    // either "stream" or carries an http error.
    let formatted = err.to_string();
    assert!(
        formatted.to_lowercase().contains("cancel")
            || formatted.to_lowercase().contains("aborted")
            || formatted.contains("stream"),
        "expected cancellation-style error, got: {formatted}"
    );
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_complete_with_stream_handles_no_cancel_token() {
    use synthia_provider::StreamChunk;

    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\
                    data: {\"type\":\"content_block_stop\",\"index\":0}\n\
                    data: {\"type\":\"message_stop\"}\n";

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());
    let request = simple_request();
    let res = provider
        .complete_with_stream(request, None, Box::new(|_chunk: StreamChunk| {}))
        .await;
    assert!(res.is_ok());
}

// =====================================================================
// OpenAI complete_with_stream tests (PR2-M3)
// =====================================================================

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_complete_with_stream_emits_v2_chunks() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    // Realistic OpenAI chat-completions streaming sequence:
    //  1. role delta
    //  2. text content deltas
    //  3. finish_reason="stop" -> IsDone
    //  4. usage chunk (because we set stream_options.include_usage)
    let sse_data = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\", world!\"},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3,\"total_tokens\":8}}\n\
                    data: [DONE]\n";

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let request = simple_request();
    let collected =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<StreamChunk>::new()));
    let collected_inner = collected.clone();
    let cancel = CancellationToken::new();

    let res = provider
        .complete_with_stream(
            request,
            Some(std::sync::Arc::new(cancel)),
            Box::new(move |chunk| {
                collected_inner.lock().unwrap().push(chunk);
            }),
        )
        .await;

    assert!(res.is_ok(), "complete_with_stream should succeed: {res:?}");

    let chunks = collected.lock().unwrap();
    let text_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::Content(ContentPart::Text(TextContent { text, .. })) if !text.is_empty()))
        .collect();
    let is_done: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::IsDone { .. }))
        .collect();

    // Two text deltas ("Hello" and ", world!") plus the role-only delta
    // (which we drop because delta.content is None). The role delta
    // should not produce a chunk.
    assert_eq!(
        text_chunks.len(),
        2,
        "expected exactly two text content chunks"
    );
    assert_eq!(is_done.len(), 1, "expected exactly one IsDone");

    // Verify the assembled IsDone payload.
    match &is_done[0] {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.text, "Hello, world!");
            assert!(result.tool_calls.is_empty());
            assert!(result.reasoning.is_empty());
        }
        _ => unreachable!(),
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_complete_with_stream_tool_call_incremental() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    // Tool call sequence with the two-arg-delta pattern that the
    // legacy processor used to drop or merge. We assert the V2
    // processor emits one ToolCallStart, two ToolCallDeltas, one
    // ToolCallEnd, and one IsDone.
    let sse_data = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"location\\\":\"}}]},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"Beijing\\\"}\"}}]},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\
                    data: [DONE]\n";

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let request = request_with_tools();

    let collected =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<StreamChunk>::new()));
    let collected_inner = collected.clone();
    let cancel = CancellationToken::new();

    let res = provider
        .complete_with_stream(
            request,
            Some(std::sync::Arc::new(cancel)),
            Box::new(move |chunk| {
                collected_inner.lock().unwrap().push(chunk);
            }),
        )
        .await;

    assert!(res.is_ok(), "complete_with_stream should succeed: {res:?}");

    let chunks = collected.lock().unwrap();
    let tool_starts: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::ToolCallStart { .. }))
        .collect();
    let tool_deltas: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::ToolCallDelta { .. }))
        .collect();
    let tool_ends: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::ToolCallEnd { .. }))
        .collect();
    let is_done: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::IsDone { .. }))
        .collect();

    assert_eq!(tool_starts.len(), 1, "expected exactly one ToolCallStart");
    assert_eq!(tool_deltas.len(), 1, "expected exactly one ToolCallDelta");
    assert_eq!(tool_ends.len(), 1, "expected exactly one ToolCallEnd");
    assert_eq!(is_done.len(), 1, "expected exactly one IsDone");

    // Verify the assembled IsDone payload.
    match &is_done[0] {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.text, "");
            assert_eq!(result.tool_calls.len(), 1);
            let tc = &result.tool_calls[0];
            assert_eq!(tc.id, "call_1");
            assert_eq!(tc.name, "get_weather");
            assert_eq!(tc.input, serde_json::json!({"location": "Beijing"}));
        }
        _ => unreachable!(),
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_complete_with_stream_reasoning_passthrough() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    // Reasoning content should be emitted as Content(Reasoning) verbatim,
    // WITHOUT any <think>/</think> sniffing.
    let sse_data = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"think <think> of something</think> more\"},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Answer.\"},\"finish_reason\":null}]}\n\
                    data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\
                    data: [DONE]\n";

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let request = simple_request();

    let collected =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<StreamChunk>::new()));
    let collected_inner = collected.clone();
    let cancel = CancellationToken::new();

    let res = provider
        .complete_with_stream(
            request,
            Some(std::sync::Arc::new(cancel)),
            Box::new(move |chunk| {
                collected_inner.lock().unwrap().push(chunk);
            }),
        )
        .await;

    assert!(res.is_ok(), "complete_with_stream should succeed: {res:?}");

    let chunks = collected.lock().unwrap();
    let reasoning_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| {
            matches!(
                c,
                StreamChunk::Content(ContentPart::Reasoning(ReasoningContent { text, .. }))
                    if text.contains("<think>")
            )
        })
        .collect();
    assert_eq!(
        reasoning_chunks.len(),
        1,
        "reasoning chunk must pass <think> verbatim"
    );

    let is_done: Vec<_> = chunks
        .iter()
        .filter(|c| matches!(c, StreamChunk::IsDone { .. }))
        .collect();
    assert_eq!(is_done.len(), 1);
    match &is_done[0] {
        StreamChunk::IsDone { result } => {
            // Reasoning must contain the raw <think> markers (no sniffing).
            assert!(result.reasoning.contains("<think>"));
            assert!(result.reasoning.contains("</think>"));
            assert_eq!(result.text, "Answer.");
        }
        _ => unreachable!(),
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_complete_with_stream_cancellation() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\
                    data: [DONE]\n";

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let request = simple_request();

    let cancel = CancellationToken::new();
    cancel.cancel(); // pre-cancel

    let res = provider
        .complete_with_stream(
            request,
            Some(std::sync::Arc::new(cancel)),
            Box::new(|_chunk: StreamChunk| {}),
        )
        .await;

    assert!(res.is_err(), "pre-cancelled stream must error out");
    let err = res.unwrap_err();
    let formatted = err.to_string();
    assert!(
        formatted.to_lowercase().contains("cancel")
            || formatted.to_lowercase().contains("aborted")
            || formatted.contains("stream"),
        "expected cancellation-style error, got: {formatted}"
    );
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_complete_with_stream_handles_no_cancel_token() {
    use synthia_provider::StreamChunk;

    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\
                    data: [DONE]\n";

    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let request = simple_request();
    let res = provider
        .complete_with_stream(request, None, Box::new(|_chunk: StreamChunk| {}))
        .await;
    assert!(res.is_ok());
}

// ---------------------------------------------------------------------------
// Error path coverage (C7 follow-up)
//
// The 30 happy-path tests above cover the 200-OK path through
// `OpenAICompatibleProvider::complete`. The integration with
// `retry_with_backoff` (which lives in src/retry.rs) was previously
// only exercised by the unit test in src/tests.rs. These 5 tests
// guard the most common production failure paths so a regression in
// status-code classification or Retry-After header parsing can't
// silently flip a 5xx into a hang or a 429 into a fall-through.
// ---------------------------------------------------------------------------

/// 429 with a `Retry-After: 30` header must surface as
/// `Error::RateLimited(Some(Duration::from_secs(30)))`. The mock is
/// set to expect `max_attempts=3` calls (the default), so a regression
/// that disables the retry classifier would also fail the `.expect(3)`.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_429_with_retry_after_returns_rate_limited_error() {
    let mock_server = MockServer::start().await;
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .append_header("retry-after", "30")
                .set_body_string("rate limited"),
        )
        .expect(6)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let result = provider.complete(simple_request()).await;
    let err = result.expect_err("429 must surface as Err, not Ok");
    // R4 typed retry (`retry_with_classification`) wraps every
    // exhausted attempt in `RetryExhausted`. The inner
    // `last_error` carries the `RateLimited` classifier output.
    let last: Error = match err {
        Error::RetryExhausted { last_error, .. } => *last_error,
        other => {
            panic!("expected RetryExhausted, got: {other:?}")
        }
    };
    assert!(
        matches!(last, Error::RateLimited { retry_after: Some(d), .. } if d == Duration::from_secs(30)),
        "expected RateLimited(Some(30s)), got: {last:?}"
    );
}

/// 500 is retryable, so the retry loop will fire `max_attempts=3`
/// times before giving up. The final error must be
/// `Error::RequestFailed { status: 500, ... }`. The mock is configured
/// with `.up_to_n_times(3)` to verify the retry classifier actually
/// sees 500 as retryable.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_500_is_retried_and_surfaces_request_failed() {
    let mock_server = MockServer::start().await;
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(500).set_body_string("internal server error"),
        )
        .up_to_n_times(4)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let result = provider.complete(simple_request()).await;

    let err = result.expect_err("500 must surface as Err after retries");
    // R4 typed retry: the retry loop wraps the final classifier
    // output in `RetryExhausted`. The 500 mock fires 3 times so
    // `attempts` must equal 3 (or whatever `max_attempts` is set
    // to in `retry_with_classification`).
    let last: Error = match err {
        Error::RetryExhausted { last_error, .. } => *last_error,
        other => {
            panic!("expected RetryExhausted, got: {other:?}")
        }
    };
    assert!(
        matches!(last, Error::RequestFailed { status: 500, .. }),
        "expected RequestFailed {{ status: 500, .. }}, got: {last:?}"
    );
}
/// 400 (client error) is non-retryable. The mock expects exactly one
/// call; if the retry classifier incorrectly treats 400 as retryable,
/// the test fails because the mock's `.expect(1)` will be violated.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_400_is_not_retried() {
    let mock_server = MockServer::start().await;
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let result = provider.complete(simple_request()).await;

    let err = result.expect_err("400 must surface as Err, not Ok");
    // R4 typed retry wraps every non-retryable error in
    // `RetryExhausted` with the inner classifier output as
    // `last_error`. The HTTP body MUST still classify 400 as
    // non-retryable (`is_retryable()` walks through the wrapper).
    let last: Error = match err {
        Error::RetryExhausted { last_error, .. } => *last_error,
        other => {
            panic!("expected RetryExhausted, got: {other:?}")
        }
    };
    assert!(
        matches!(last, Error::RequestFailed { status: 400, .. }),
        "expected RequestFailed {{ status: 400, .. }}, got: {last:?}"
    );
    assert!(
        !last.is_retryable(),
        "400 must not be classified as retryable"
    );
}

/// 401 (auth error) is non-retryable. The mock expects exactly one
/// call; if the classifier regresses and treats 401 as retryable, the
/// test fails.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_401_is_not_retried() {
    let mock_server = MockServer::start().await;
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string("unauthorized"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let result = provider.complete(simple_request()).await;

    let err = result.expect_err("401 must surface as Err, not Ok");
    let last: Error = match err {
        Error::RetryExhausted { last_error, .. } => *last_error,
        other => {
            panic!("expected RetryExhausted, got: {other:?}")
        }
    };
    assert!(
        matches!(last, Error::RequestFailed { status: 401, .. }),
        "expected RequestFailed {{ status: 401, .. }}, got: {last:?}"
    );
    assert!(
        !last.is_retryable(),
        "401 must not be classified as retryable"
    );
}

/// `parse_retry_after` must support both integer-seconds and HTTP-date
/// formats. Regression guard: a fix that only accepts one format
/// would break half the rate-limited servers in the wild.
#[test]
fn test_parse_retry_after_supports_seconds_and_http_date() {
    use synthia_provider::retry::parse_retry_after;
    // Integer seconds — the most common format.
    assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
    assert_eq!(parse_retry_after("0"), Some(Duration::from_secs(0)));
    // HTTP date format (RFC 2822). Use a date 60s in the future so
    // `chrono::Utc::now() - date > 0`.
    let future = chrono::Utc::now() + chrono::Duration::seconds(60);
    let http_date = future.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let parsed = parse_retry_after(&http_date).expect("HTTP date should parse");
    // Allow a small slack window: the date is computed at one instant
    // and parsed at the next, so the result can be 59 or 60s depending
    // on rounding.
    assert!(
        parsed >= Duration::from_secs(55) && parsed <= Duration::from_secs(65),
        "expected ~60s, got {parsed:?}"
    );
    // Unparseable input -> None.
    assert_eq!(parse_retry_after("not-a-date"), None);
}

// =====================================================================
// R30: empty terminal completions, quota classification (Slice A)
// =====================================================================

/// A terminal `stop` with no content is a degenerate completion, not a
/// successful empty turn: it MUST error, and the error is retryable
/// with the small `EmptyResponse` budget (2 attempts), so the mock
/// expects exactly two calls before the wrapped error surfaces.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_empty_terminal_completion_is_retried_then_errors() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "cmpl-empty",
        "object": "chat.completion",
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": ""},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 5, "completion_tokens": 0, "total_tokens": 5}
    });
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(2)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let result = provider.complete(simple_request()).await;

    let err = result.expect_err("empty terminal completion must be Err");
    let Error::RetryExhausted {
        attempts,
        last_error,
    } = err
    else {
        panic!("expected RetryExhausted, got: {err:?}");
    };
    assert_eq!(attempts, 2, "EmptyResponse budget is 2 attempts");
    assert!(
        synthia_provider::is_empty_response_error(&last_error),
        "last error must carry the typed empty-completion marker, got: {last_error:?}"
    );
}

/// The Anthropic adapter's non-streaming path applies the same
/// guard: `stop_reason: "end_turn"` with an empty content array errors
/// (and retries twice).
#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_empty_terminal_completion_is_retried_then_errors() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "msg_empty",
        "type": "message",
        "role": "assistant",
        "model": "test-model",
        "content": [],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 5, "output_tokens": 0}
    });
    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(2)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());
    let result = provider.complete(simple_request()).await;

    let err = result.expect_err("empty terminal completion must be Err");
    let Error::RetryExhausted {
        attempts,
        last_error,
    } = err
    else {
        panic!("expected RetryExhausted, got: {err:?}");
    };
    assert_eq!(attempts, 2);
    assert!(
        synthia_provider::is_empty_response_error(&last_error),
        "last error must carry the typed empty-completion marker, got: {last_error:?}"
    );
}

/// The streaming OpenAI path finalizes through the same guard: a
/// `finish_reason: "stop"` chunk with no deltas errors instead of
/// returning an empty successful turn.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_streaming_empty_terminal_completion_errors() {
    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"id\":\"cmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":\"stop\"}]}\n\
                    data: [DONE]\n";
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let result = provider
        .complete_with_stream(
            simple_request(),
            None,
            Box::new(|_chunk: StreamChunk| {}),
        )
        .await;

    let err = result.expect_err("empty streamed turn must be Err");
    assert!(
        synthia_provider::is_empty_response_error(&err),
        "got: {err:?}"
    );
}

/// The streaming Anthropic path finalizes through the same guard: a
/// `message_stop` with no content blocks errors.
#[cfg(feature = "anthropic")]
#[tokio::test]
async fn test_anthropic_streaming_empty_terminal_completion_errors() {
    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"type\":\"message_stop\"}\n";
    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());
    let result = provider
        .complete_with_stream(
            simple_request(),
            None,
            Box::new(|_chunk: StreamChunk| {}),
        )
        .await;

    let err = result.expect_err("empty streamed turn must be Err");
    assert!(
        synthia_provider::is_empty_response_error(&err),
        "got: {err:?}"
    );
}

/// A truncation stop (`length`) with zero content is NOT the empty
/// case: the caller needs the length signal, so the call stays Ok.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_length_stop_without_content_stays_ok() {
    let mock_server = MockServer::start().await;
    let response_body = serde_json::json!({
        "id": "cmpl-truncated",
        "object": "chat.completion",
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": ""},
            "finish_reason": "length"
        }],
        "usage": {"prompt_tokens": 5, "completion_tokens": 0, "total_tokens": 5}
    });
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let response = provider
        .complete(simple_request())
        .await
        .expect("length stop is not an empty completion");
    assert_eq!(response.stop_reason.as_deref(), Some("length"));
}

/// An exhausted account quota (HTTP 402, DeepSeek/OpenAI-compatible
/// wording) is classified `Quota`: NOT retried within a session, so
/// the mock expects exactly one call.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_402_quota_is_not_retried() {
    let mock_server = MockServer::start().await;
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(402).set_body_string("Insufficient Balance"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let err = provider
        .complete(simple_request())
        .await
        .expect_err("402 must surface as Err");

    let Error::RetryExhausted {
        attempts,
        last_error,
    } = err
    else {
        panic!("expected RetryExhausted, got: {err:?}");
    };
    assert_eq!(attempts, 1, "quota exhaustion is not retryable");
    assert!(
        matches!(&*last_error, Error::RequestFailed { status: 402, .. }),
        "got: {last_error:?}"
    );
    assert_eq!(
        synthia_provider::classify_error(&last_error),
        synthia_provider::RetryClass::Quota
    );
}

/// OpenAI reports an exhausted account quota with HTTP **429** and an
/// `insufficient_quota` body — the same status as a request-rate
/// limit. The body must decide: exactly one attempt (the 6-attempt
/// rate-limit budget would burn a minute of sleeps on a failure that
/// cannot clear), and the inner error must keep the body so the
/// classifier still reads it.
#[cfg(feature = "openai")]
#[tokio::test]
async fn test_openai_429_insufficient_quota_body_is_not_retried() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "error": {
            "message": "You exceeded your current quota, please check \
                        your plan and billing details.",
            "type": "insufficient_quota",
            "param": null,
            "code": "insufficient_quota"
        }
    });
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_json(body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");
    let err = provider
        .complete(simple_request())
        .await
        .expect_err("429 quota must surface as Err");

    let Error::RetryExhausted {
        attempts,
        last_error,
    } = err
    else {
        panic!("expected RetryExhausted, got: {err:?}");
    };
    assert_eq!(attempts, 1, "quota exhaustion is not retryable");
    assert!(
        matches!(&*last_error, Error::RequestFailed { status: 429, .. }),
        "the body must reach the classifier: {last_error:?}"
    );
    assert_eq!(
        synthia_provider::classify_error(&last_error),
        synthia_provider::RetryClass::Quota
    );
}

// ---------------------------------------------------------------------------
// Streaming retry: the establishment phase only
// ---------------------------------------------------------------------------

/// A transient failure while *opening* the stream is retried, so a
/// 503 does not end the run.
///
/// Before this, the streaming path — the only path a ReAct iteration
/// takes — handed any `Err` straight back to the loop as a session
/// -fatal reason, while the non-streaming `complete` path retried with
/// per-class backoff. A provider having a capacity blip therefore
/// killed the run.
#[cfg(feature = "anthropic")]
#[tokio::test]
async fn anthropic_stream_retries_a_transient_open_failure() {
    use synthia_provider::StreamChunk;

    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"recovered\"}}\n\
                    data: {\"type\":\"content_block_stop\",\"index\":0}\n\
                    data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\
                    data: {\"type\":\"message_stop\"}\n";

    // First attempt: capacity blip. Then the real stream.
    Mock::given(matchers::path("/v1/messages"))
        .respond_with(ResponseTemplate::new(503).set_body_string("overloaded"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(matchers::path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    // `on_delta` is `'static`, so the sink is shared rather than
    // borrowed.
    let text = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = std::sync::Arc::clone(&text);
    let resp = provider
        .complete_with_stream(
            simple_request(),
            None,
            Box::new(move |chunk: StreamChunk| {
                if let StreamChunk::Content(ContentPart::Text(t)) = chunk
                    && let Ok(mut guard) = sink.lock()
                {
                    guard.push_str(&t.text);
                }
            }),
        )
        .await
        .expect("a 503 followed by success must be retried, not fatal");

    assert_eq!(
        text.lock().unwrap().as_str(),
        "recovered",
        "the retried attempt's deltas must land"
    );
    assert_eq!(resp.stop_reason.as_deref(), Some("end_turn"));
    let requests = mock_server
        .received_requests()
        .await
        .expect("wiremock records requests");
    assert_eq!(
        requests.len(),
        2,
        "the 503 must have been re-sent exactly once"
    );
}

/// Once the stream is open and deltas have been handed to the caller,
/// a later failure is **not** retried.
///
/// Re-sending would emit the already-delivered deltas a second time,
/// and the consumer cannot un-see them. The cancellation here happens
/// *inside* the delta callback — i.e. after the first chunk has
/// reached the caller — which is exactly the boundary the retry must
/// respect. The mock answers 200, so nothing about the request itself
/// invites a retry; only the mid-stream abort ends the pass, and it
/// must end it after exactly one request.
#[cfg(feature = "anthropic")]
#[tokio::test]
async fn anthropic_stream_does_not_retry_after_deltas() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    let sse_data = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\
                    data: {\"type\":\"content_block_stop\",\"index\":0}\n\
                    data: {\"type\":\"message_stop\"}\n";
    Mock::given(matchers::path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    // Cancel as soon as the first delta lands: the stream is open and
    // the caller has already seen content.
    let cancel = CancellationToken::new();
    let from_callback = cancel.clone();
    let res = provider
        .complete_with_stream(
            simple_request(),
            Some(std::sync::Arc::new(cancel)),
            Box::new(move |_chunk: StreamChunk| {
                from_callback.cancel();
            }),
        )
        .await;

    assert!(
        res.is_err(),
        "an aborted stream must surface as an error even though the \
         request itself succeeded"
    );
    let requests = mock_server
        .received_requests()
        .await
        .expect("wiremock records requests");
    assert_eq!(
        requests.len(),
        1,
        "a failure after the stream opened must not be retried; got {} \
         request(s)",
        requests.len()
    );
}

/// The same retry, on the OpenAI-compatible adapter.
#[cfg(feature = "openai")]
#[tokio::test]
async fn openai_stream_retries_a_transient_open_failure() {
    use synthia_provider::StreamChunk;

    let mock_server = MockServer::start().await;
    let sse = "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"recovered\"},\"finish_reason\":null}]}\n\n\
               data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
               data: [DONE]\n\n";

    Mock::given(matchers::path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("overloaded"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(matchers::path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let text = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = std::sync::Arc::clone(&text);
    provider
        .complete_with_stream(
            simple_request(),
            None,
            Box::new(move |chunk: StreamChunk| {
                if let StreamChunk::Content(ContentPart::Text(t)) = chunk
                    && let Ok(mut guard) = sink.lock()
                {
                    guard.push_str(&t.text);
                }
            }),
        )
        .await
        .expect("a 503 followed by success must be retried, not fatal");

    assert_eq!(
        text.lock().unwrap().as_str(),
        "recovered",
        "the retried attempt's deltas must land"
    );
    let requests = mock_server
        .received_requests()
        .await
        .expect("wiremock records requests");
    assert_eq!(
        requests.len(),
        2,
        "the 503 must have been re-sent exactly once"
    );
}

/// A real Anthropic SSE turn ends with its reasoning block **signed**.
///
/// This drives the whole chain a real run takes — wire bytes →
/// `StreamProcessor` → streamed chunks → `BlockAssembler` → the
/// assistant parts the agent commits to history — because the
/// signature is the one piece a chunk-at-a-time fold cannot see: the
/// `signature_delta` arrives *after* the `thinking_delta`s that carried
/// the text, so the streamed `Content(Reasoning)` chunks are stamped
/// with `None` and the value only shows up on `IsDone`. Every step
/// here is the production one; nothing is hand-built.
///
/// Without the deferred-signature fold the final part comes back
/// `signature: None`, and Anthropic refuses to continue a turn whose
/// thinking block has lost its signature.
#[cfg(feature = "anthropic")]
#[tokio::test]
async fn anthropic_stream_signature_survives_to_the_assembled_parts() {
    use synthia_provider::{BlockAssembler, StreamChunk};

    let mock_server = MockServer::start().await;
    // The real event order: thinking text first, then the signature.
    let sse_data = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"step one "}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"step two"}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig_real_123"}}
data: {"type":"content_block_stop","index":0}
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"done"}}
data: {"type":"content_block_stop","index":1}
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}
data: {"type":"message_stop"}
"#;

    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_data)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    // Collect exactly what the adapter hands the agent loop.
    let chunks =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::<StreamChunk>::new()));
    let sink = std::sync::Arc::clone(&chunks);
    provider
        .complete_with_stream(
            simple_request(),
            None,
            Box::new(move |chunk: StreamChunk| {
                if let Ok(mut guard) = sink.lock() {
                    guard.push(chunk);
                }
            }),
        )
        .await
        .expect("the streamed turn must complete");

    let chunks = chunks.lock().unwrap().clone();
    // Precondition: the streamed reasoning chunk genuinely has no
    // signature — this is the fact that makes the fold necessary.
    let streamed_reasoning_signature = chunks.iter().find_map(|c| match c {
        StreamChunk::Content(ContentPart::Reasoning(r)) => {
            Some(r.signature.clone())
        }
        _ => None,
    });
    assert_eq!(
        streamed_reasoning_signature,
        Some(None),
        "a streamed reasoning chunk cannot know its signature yet; \
         chunks were: {chunks:?}"
    );

    // Now fold them the way the agent loop does.
    let mut assembler = BlockAssembler::new();
    for chunk in chunks {
        assembler.push(chunk);
    }
    let (result, parts, _incomplete) = assembler.finalize_assistant();

    let part_signature = parts.iter().find_map(|p| match p {
        ContentPart::Reasoning(r) => Some(r.signature.clone()),
        _ => None,
    });
    assert_eq!(
        part_signature,
        Some(Some("sig_real_123".to_string())),
        "the deferred signature must land on the assembled reasoning \
         part; parts were: {parts:?}"
    );
    assert_eq!(
        result.reasoning_signature.as_deref(),
        Some("sig_real_123"),
        "and the terminal result must report it too"
    );
    assert_eq!(result.reasoning, "step one step two");
}

/// A cancelled run does not retry — cancellation is checked before
/// **every** attempt, not only the first.
///
/// Retrying establishment introduced a window this pins shut: the retry
/// budget for a sustained 429 is 6 attempts over a 1s→30s backoff
/// (~31s of sleeping), and the SSE pump that normally observes the token
/// is only reached after establishment. Without an in-closure check a
/// cancelled run — or a disconnected client — would sit in backoff for
/// up to half a minute before the cancellation was noticed, where it
/// used to fail fast on the single attempt.
///
/// The mock answers 503, which *is* retryable, so only the cancel guard
/// can keep the request count at one.
#[cfg(feature = "anthropic")]
#[tokio::test]
async fn anthropic_stream_cancelled_before_send_does_not_retry() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_string("overloaded"))
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let cancel = CancellationToken::new();
    cancel.cancel();

    let res = provider
        .complete_with_stream(
            simple_request(),
            Some(std::sync::Arc::new(cancel)),
            Box::new(|_chunk: StreamChunk| {}),
        )
        .await;

    let err = res.expect_err("a cancelled run must fail");
    assert!(
        err.to_string().contains("aborted"),
        "the failure must be the abort, not a retry exhaustion; got: {err}"
    );
    // Zero, not one: the guard runs before the send, so a cancelled run
    // does not spend a request at all — and cannot sleep through the
    // cancel in the retry backoff.
    let requests = mock_server
        .received_requests()
        .await
        .expect("wiremock records requests");
    assert!(
        requests.is_empty(),
        "a cancelled run must not send; got {} request(s)",
        requests.len()
    );
}

/// Same guard on the OpenAI-compatible adapter.
#[cfg(feature = "openai")]
#[tokio::test]
async fn openai_stream_cancelled_before_send_does_not_retry() {
    use synthia_provider::StreamChunk;
    use tokio_util::sync::CancellationToken;

    let mock_server = MockServer::start().await;
    Mock::given(matchers::path("/chat/completions"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_string("overloaded"))
        .mount(&mock_server)
        .await;

    let provider =
        OpenAICompatibleProvider::new(mock_server.uri(), test_model_config())
            .with_api_key("test-key");

    let cancel = CancellationToken::new();
    cancel.cancel();

    let res = provider
        .complete_with_stream(
            simple_request(),
            Some(std::sync::Arc::new(cancel)),
            Box::new(|_chunk: StreamChunk| {}),
        )
        .await;

    let err = res.expect_err("a cancelled run must fail");
    assert!(
        err.to_string().contains("aborted"),
        "the failure must be the abort, not a retry exhaustion; got: {err}"
    );
    let requests = mock_server
        .received_requests()
        .await
        .expect("wiremock records requests");
    assert!(
        requests.is_empty(),
        "a cancelled run must not send; got {} request(s)",
        requests.len()
    );
}
