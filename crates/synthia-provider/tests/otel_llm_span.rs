#![cfg(feature = "otel")]
//! Integration tests for the `llm.call` span emitted by
//! `AnthropicProvider::complete` and `OpenAICompatibleProvider::complete`
//! when the `otel` cargo feature is enabled.
//!
//! Verifies the span records the OTel GenAI semantic-convention
//! attributes:
//!  - `gen_ai.system` / `gen_ai.request.model` at span creation
//!  - `gen_ai.response.finish_reason` / `gen_ai.usage.input_tokens` /
//!    `gen_ai.usage.output_tokens` on success
//!  - `exception.type` / `exception.message` / `otel.status_code` on
//!    failure (uses HTTP 400, which is non-retryable, so
//!    `retry_with_backoff` returns immediately without sleeping)
//!
//! The capture layer records both the initial fields (via
//! `on_new_span`) and the fields populated later via `Span::record`
//! (via `on_record`). Field values are stringified through the
//! `tracing::field::Visit::record_debug` trait — string fields
//! recorded with `%` (Display) are captured without surrounding
//! quotes, while `&str` / `usize` values are captured with Debug
//! formatting. All assertions use `contains(...)` for robustness.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use synthia_provider::{
    AnthropicProvider,
    ModelProvider,
    OpenAICompatibleProvider,
    types::{
        CompletionRequest,
        Content,
        ContentPart,
        Message,
        ModelConfig,
        Role,
        TextContent,
        ToolChoice,
    },
};
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

#[derive(Default, Debug)]
struct CapturedSpan {
    name: String,
    fields: HashMap<String, String>,
}

struct CaptureLayer {
    spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
}

struct FieldVisitor<'a>(&'a mut HashMap<String, String>);

impl tracing::field::Visit for FieldVisitor<'_> {
    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
    ) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let mut span = CapturedSpan {
            name: attrs.metadata().name().to_string(),
            fields: HashMap::new(),
        };
        let mut visitor = FieldVisitor(&mut span.fields);
        attrs.record(&mut visitor);
        self.spans.lock().unwrap().insert(id.clone(), span);
    }

    fn on_record(
        &self,
        span: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: Context<'_, S>,
    ) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(span_data) = spans.get_mut(span) {
            let mut visitor = FieldVisitor(&mut span_data.fields);
            values.record(&mut visitor);
        }
    }
}

/// Install a `CaptureLayer` as the thread-local default subscriber and
/// return the shared `spans` map for assertions. The returned guard
/// keeps the subscriber alive until dropped (must outlive the
/// `complete()` call under test).
fn install_capture_layer() -> (
    Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
    tracing::subscriber::DefaultGuard,
) {
    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();
    (spans, guard)
}

/// Extract the single captured span named `llm.call` from the map.
/// Panics if zero or more than one `llm.call` span was captured.
fn expect_llm_call_span(
    spans: &HashMap<tracing::span::Id, CapturedSpan>,
) -> &CapturedSpan {
    let llm_spans: Vec<_> =
        spans.values().filter(|s| s.name == "llm.call").collect();
    assert_eq!(
        llm_spans.len(),
        1,
        "expected exactly one llm.call span, got {}: {:?}",
        llm_spans.len(),
        llm_spans.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    llm_spans[0]
}

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
    }
}

fn assert_field_contains(
    fields: &HashMap<String, String>,
    name: &str,
    needle: &str,
) {
    match fields.get(name) {
        Some(v) => assert!(
            v.contains(needle),
            "field `{name}` should contain {needle:?}, got {v:?}"
        ),
        None => panic!(
            "field `{name}` should be present, but was not recorded. \
             Captured fields: {fields:?}"
        ),
    }
}

fn assert_field_present(fields: &HashMap<String, String>, name: &str) {
    assert!(
        fields.get(name).is_some(),
        "field `{name}` should be present, but was not recorded. \
         Captured fields: {fields:?}"
    );
}

fn assert_field_absent(fields: &HashMap<String, String>, name: &str) {
    assert!(
        fields.get(name).is_none(),
        "field `{name}` should NOT be recorded, but got: {:?}. \
         Captured fields: {fields:?}",
        fields.get(name)
    );
}

// =====================================================================
// Anthropic success + failure
// =====================================================================

#[tokio::test]
async fn test_anthropic_complete_success_span_has_gen_ai_attributes() {
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

    let (spans, _guard) = install_capture_layer();
    let request = simple_request();
    let result = provider.complete(request).await;
    assert!(
        result.is_ok(),
        "complete() should succeed: {:?}",
        result.err()
    );

    let spans = spans.lock().unwrap();
    let span = expect_llm_call_span(&spans);
    let fields = &span.fields;

    // Attributes set at span creation
    assert_field_contains(fields, "gen_ai.system", "anthropic");
    assert_field_contains(fields, "gen_ai.request.model", "test-model");
    // Attributes recorded on success
    assert_field_contains(fields, "gen_ai.response.finish_reason", "end_turn");
    assert_field_contains(fields, "gen_ai.usage.input_tokens", "10");
    assert_field_contains(fields, "gen_ai.usage.output_tokens", "20");
    // No exception attributes on success
    assert_field_absent(fields, "exception.type");
    assert_field_absent(fields, "exception.message");
    assert_field_absent(fields, "otel.status_code");
}

#[tokio::test]
async fn test_anthropic_complete_failure_span_records_exception() {
    let mock_server = MockServer::start().await;
    // 400 is non-retryable — retry_with_backoff returns immediately
    // without sleeping, so this test is fast.
    Mock::given(matchers::path("/v1/messages"))
        .and(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let provider = AnthropicProvider::new(test_model_config())
        .with_api_key("test-key")
        .with_base_url(&mock_server.uri());

    let (spans, _guard) = install_capture_layer();
    let request = simple_request();
    let result = provider.complete(request).await;
    assert!(
        result.is_err(),
        "400 should surface as Err, got: {result:?}"
    );

    let spans = spans.lock().unwrap();
    let span = expect_llm_call_span(&spans);
    let fields = &span.fields;

    // Attributes set at span creation (even on failure)
    assert_field_contains(fields, "gen_ai.system", "anthropic");
    assert_field_contains(fields, "gen_ai.request.model", "test-model");
    // Exception attributes recorded on failure
    assert_field_present(fields, "exception.type");
    assert_field_present(fields, "exception.message");
    assert_field_contains(fields, "otel.status_code", "ERROR");
    // No success attributes on failure
    assert_field_absent(fields, "gen_ai.response.finish_reason");
    assert_field_absent(fields, "gen_ai.usage.input_tokens");
    assert_field_absent(fields, "gen_ai.usage.output_tokens");
}

// =====================================================================
// OpenAI success + failure
// =====================================================================

#[tokio::test]
async fn test_openai_complete_success_span_has_gen_ai_attributes() {
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

    let (spans, _guard) = install_capture_layer();
    let request = simple_request();
    let result = provider.complete(request).await;
    assert!(
        result.is_ok(),
        "complete() should succeed: {:?}",
        result.err()
    );

    let spans = spans.lock().unwrap();
    let span = expect_llm_call_span(&spans);
    let fields = &span.fields;

    // gen_ai.system uses model_config.provider (which is "test" in
    // test_model_config), NOT self.name() (which returns the model
    // name for OpenAI).
    assert_field_contains(fields, "gen_ai.system", "test");
    assert_field_contains(fields, "gen_ai.request.model", "test-model");
    // Attributes recorded on success
    assert_field_contains(fields, "gen_ai.response.finish_reason", "stop");
    assert_field_contains(fields, "gen_ai.usage.input_tokens", "10");
    assert_field_contains(fields, "gen_ai.usage.output_tokens", "20");
    // No exception attributes on success
    assert_field_absent(fields, "exception.type");
    assert_field_absent(fields, "exception.message");
    assert_field_absent(fields, "otel.status_code");
}

#[tokio::test]
async fn test_openai_complete_failure_span_records_exception() {
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

    let (spans, _guard) = install_capture_layer();
    let request = simple_request();
    let result = provider.complete(request).await;
    assert!(
        result.is_err(),
        "400 should surface as Err, got: {result:?}"
    );

    let spans = spans.lock().unwrap();
    let span = expect_llm_call_span(&spans);
    let fields = &span.fields;

    // Attributes set at span creation (even on failure)
    assert_field_contains(fields, "gen_ai.system", "test");
    assert_field_contains(fields, "gen_ai.request.model", "test-model");
    // Exception attributes recorded on failure
    assert_field_present(fields, "exception.type");
    assert_field_present(fields, "exception.message");
    assert_field_contains(fields, "otel.status_code", "ERROR");
    // No success attributes on failure
    assert_field_absent(fields, "gen_ai.response.finish_reason");
    assert_field_absent(fields, "gen_ai.usage.input_tokens");
    assert_field_absent(fields, "gen_ai.usage.output_tokens");
}
