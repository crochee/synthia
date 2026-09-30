//! End-to-end tests for the Anthropic Messages surface
//! (`POST /v1/messages`).
//!
//! The contract is documented on `routes::messages`:
//!
//! - the route answers with the protocol's own shapes — a
//!   `{id, type:"message", role:"assistant", …}` body, an SSE stream of
//!   `message_start` → `content_block_start` → `content_block_delta`* →
//!   `content_block_stop` → `message_delta` → `message_stop`, and
//!   `{"type":"error","error":{…}}` for every failure;
//! - it shares the chat surface's run machinery, so a request really
//!   does drive an agent run (here against a deterministic
//!   `FakeProvider`);
//! - mounting it does not shadow `/api/v1/*`.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use synthia::{
    provider::{
        CompletionRequest,
        CompletionResponse,
        Content,
        ContentPart,
        Message,
        ModelConfig,
        ModelProvider,
        ProviderConfig,
        ResourceLink,
        Role,
    },
    test_support::FakeProvider,
};
use synthia_server::{
    create_router,
    state::{AppState, ProviderCatalogue},
};
use tower::ServiceExt;

/// The answer the fake provider returns; every assertion below reads it
/// back through the protocol.
const ANSWER: &str = "Hello from Synthia";

/// Temp dir + router + state, with a provider that answers
/// deterministically.
struct Fixture {
    app: axum::Router,
    /// Held so the on-disk session sinks outlive the router.
    _temp: tempfile::TempDir,
}

async fn make_fixture() -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let registry = synthia::session::manager::SessionRegistry::new(
        temp.path().join("sessions"),
    );
    let mut state =
        AppState::for_test(registry, temp.path().to_path_buf()).await;
    // `for_test` wires an empty-response provider (enough for route
    // wiring, useless for a run). Replace it before any controller is
    // built: controllers snapshot the provider at creation.
    let provider: Arc<dyn ModelProvider> = Arc::new(FakeProvider::text(ANSWER));
    state.default_provider = Arc::clone(&provider);
    // The catalogue a request's `model` resolves against. Resolution is
    // strict — a value no configured provider advertises is a `4xx` —
    // and these requests name a bare model id, so the fixture registers
    // the one it posts.
    state.provider_catalogue =
        Arc::new(ProviderCatalogue::default().with_handle(
            "anthropic",
            "claude-compat-test",
            provider,
        ));
    Fixture {
        app: create_router(Arc::new(state)).await,
        _temp: temp,
    }
}

/// POST a raw body to `/v1/messages`.
async fn post(fix: &Fixture, body: &str) -> (StatusCode, Value) {
    let response = fix
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("x-api-key", "test-key")
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "response body must be JSON ({error}): {:?}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, json)
}

fn request_body(stream: bool) -> String {
    json!({
        "model": "claude-compat-test",
        "max_tokens": 64,
        "messages": [{ "role": "user", "content": "hi" }],
        "stream": stream,
    })
    .to_string()
}

/// One SSE frame as it appears on the wire.
#[derive(Debug)]
struct SseFrame {
    event: String,
    data: Value,
}

/// Parse a complete SSE body into frames.
///
/// The response stream is finite — it ends at the run's terminal event,
/// unlike the chat surface's live tail — so the body is collected
/// whole.
fn parse_sse(bytes: &[u8]) -> Vec<SseFrame> {
    let text = String::from_utf8(bytes.to_vec()).expect("SSE is UTF-8");
    let mut frames = Vec::new();
    for block in text.split("\n\n").filter(|block| !block.trim().is_empty()) {
        let mut event = None;
        let mut data = None;
        for line in block.lines() {
            if let Some(name) = line.strip_prefix("event: ") {
                event = Some(name.to_string());
            } else if let Some(payload) = line.strip_prefix("data: ") {
                data = Some(serde_json::from_str(payload).unwrap_or_else(
                    |error| panic!("frame data ({error}): {payload}"),
                ));
            }
        }
        frames.push(SseFrame {
            event: event
                .unwrap_or_else(|| panic!("frame without `event:`: {block:?}")),
            data: data
                .unwrap_or_else(|| panic!("frame without `data:`: {block:?}")),
        });
    }
    frames
}

/// A plain request runs the agent and answers with one complete
/// Anthropic message — never the application's `{code,message}`
/// envelope.
#[tokio::test]
async fn non_streaming_request_returns_an_anthropic_message() {
    let fix = make_fixture().await;
    let (status, body) = post(&fix, &request_body(false)).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["type"], "message");
    assert_eq!(body["role"], "assistant");
    assert_eq!(body["model"], "claude-compat-test");
    assert!(
        body["id"].as_str().unwrap_or_default().starts_with("msg_"),
        "a message id must use the protocol's prefix: {body}"
    );
    assert_eq!(body["stop_reason"], "end_turn");
    assert!(body["stop_sequence"].is_null());
    assert_eq!(body["content"][0]["type"], "text");
    assert_eq!(
        body["content"][0]["text"], ANSWER,
        "the agent's answer must come back as a text block"
    );
    assert!(
        body["usage"]["output_tokens"].is_number()
            && body["usage"]["input_tokens"].is_number(),
        "usage must carry both token counts: {body}"
    );
    assert!(
        body.get("code").is_none(),
        "the application envelope must never appear here: {body}"
    );
}

/// `"stream": true` emits the protocol's event sequence, in order, with
/// each frame carrying its own payload.
#[tokio::test]
async fn streaming_request_emits_the_anthropic_event_sequence() {
    let fix = make_fixture().await;
    let response = fix
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("x-api-key", "test-key")
                .header("content-type", "application/json")
                .body(Body::from(request_body(true)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default(),
        "text/event-stream",
        "a streamed turn must be served as SSE"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(
        bytes.starts_with(b"event: message_start\ndata: "),
        "the first frame must be a framed message_start: {:?}",
        String::from_utf8_lossy(&bytes[..bytes.len().min(80)])
    );

    let frames = parse_sse(&bytes);
    let names: Vec<&str> =
        frames.iter().map(|frame| frame.event.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ],
        "the protocol's event order is fixed"
    );

    let start = &frames[0].data;
    assert_eq!(start["type"], "message_start");
    assert_eq!(start["message"]["role"], "assistant");
    assert_eq!(start["message"]["model"], "claude-compat-test");
    assert_eq!(
        start["message"]["content"],
        json!([]),
        "message_start carries an empty content array"
    );
    assert!(start["message"]["stop_reason"].is_null());

    assert_eq!(frames[1].data["type"], "content_block_start");
    assert_eq!(frames[1].data["index"], 0);
    assert_eq!(
        frames[1].data["content_block"],
        json!({ "type": "text", "text": "" }),
        "a block starts empty and grows by delta"
    );

    assert_eq!(frames[2].data["type"], "content_block_delta");
    assert_eq!(frames[2].data["index"], 0);
    assert_eq!(
        frames[2].data["delta"],
        json!({ "type": "text_delta", "text": ANSWER })
    );

    assert_eq!(frames[3].data["type"], "content_block_stop");
    assert_eq!(frames[3].data["index"], 0);

    assert_eq!(frames[4].data["type"], "message_delta");
    assert_eq!(frames[4].data["delta"]["stop_reason"], "end_turn");
    assert!(frames[4].data["usage"]["output_tokens"].is_number());

    assert_eq!(frames[5].data["type"], "message_stop");
}

/// A body the endpoint cannot use answers `400` with the protocol's
/// error envelope, not the application's. Each message names what is
/// wrong, so a client can act on it rather than guess.
#[tokio::test]
async fn malformed_bodies_return_the_anthropic_error_envelope() {
    let fix = make_fixture().await;
    for (label, body, named) in [
        ("not json at all", "{oops", "invalid request body"),
        (
            "missing max_tokens",
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#,
            "max_tokens",
        ),
        (
            "assistant prefill",
            r#"{"model":"m","max_tokens":8,"messages":[{"role":"assistant","content":"hi"}]}"#,
            "the final message must have role",
        ),
        (
            "unknown document source",
            r#"{"model":"m","max_tokens":8,"messages":[{"role":"user","content":[{"type":"document","source":{"type":"file","file_id":"f1"}}]}]}"#,
            "unknown variant `file`",
        ),
        (
            "document without a source",
            r#"{"model":"m","max_tokens":8,"messages":[{"role":"user","content":[{"type":"document"}]}]}"#,
            "missing field `source`",
        ),
        (
            "document source without its bytes",
            r#"{"model":"m","max_tokens":8,"messages":[{"role":"user","content":[{"type":"document","source":{"type":"base64","media_type":"application/pdf"}}]}]}"#,
            "missing field `data`",
        ),
    ] {
        let (status, body) = post(&fix, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{label}: {body}");
        assert_eq!(body["type"], "error", "{label}: {body}");
        assert_eq!(
            body["error"]["type"], "invalid_request_error",
            "{label}: {body}"
        );
        let message = body["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains(named),
            "{label} must name the problem ({named:?}): {body}"
        );
        assert!(
            body.get("code").is_none(),
            "{label} must not leak the application envelope: {body}"
        );
    }
}

/// Mounting the compatibility route must not shadow the management
/// nest: a known `/api/v1/...` path still reaches its handler.
#[tokio::test]
async fn management_routes_still_resolve() {
    let fix = make_fixture().await;

    let models = fix
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        models.status(),
        StatusCode::OK,
        "/api/v1/models must still route"
    );

    let created = fix
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/chat/sessions")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "session_id": "smoke" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        created.status(),
        StatusCode::OK,
        "/api/v1/chat/sessions must still route"
    );
    let bytes = created.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["session_id"], "smoke");
}

/// With auth unconfigured the server's existing dev path applies, so an
/// Anthropic client that sends `x-api-key` (and no `Authorization`)
/// talks to it out of the box.
#[tokio::test]
async fn x_api_key_request_succeeds_when_auth_is_unconfigured() {
    let fix = make_fixture().await;
    let (status, body) = post(&fix, &request_body(false)).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["type"], "message");
}

/// The chat surface's SSE route and the compatibility route coexist.
#[tokio::test]
async fn chat_stream_route_still_resolves() {
    let fix = make_fixture().await;
    let response = fix
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/chat/sessions/absent/messages/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Any status but 404 proves the route is mounted; the resume cursor
    // semantics are pinned by `stream_resume_test`.
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the chat SSE route must stay mounted"
    );
}

/// A provider that records what the agent asked it, so the request's
/// history can be inspected from the outside.
#[derive(Debug, Default)]
struct CapturingProvider {
    captured: parking_lot::Mutex<Vec<Vec<Message>>>,
}

impl CapturingProvider {
    fn requests(&self) -> Vec<Vec<Message>> {
        self.captured.lock().clone()
    }
}

#[async_trait::async_trait]
impl ModelProvider for CapturingProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), synthia::core::Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "capturing"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "capturing-model".to_string(),
            provider: "capturing".to_string(),
            context_window: 128_000,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, synthia::core::Error> {
        self.captured.lock().push(request.messages.as_ref().clone());
        Ok(CompletionResponse {
            content: Content::text("captured"),
            ..CompletionResponse::default()
        })
    }

    async fn embed(
        &self,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, synthia::core::Error> {
        Ok(Vec::new())
    }
}

/// Router + provider for a test that inspects what the model was asked,
/// with the temp dir the session sinks live in.
async fn capturing_fixture()
-> (axum::Router, Arc<CapturingProvider>, tempfile::TempDir) {
    let temp = tempfile::TempDir::new().unwrap();
    let registry = synthia::session::manager::SessionRegistry::new(
        temp.path().join("sessions"),
    );
    let mut state =
        AppState::for_test(registry, temp.path().to_path_buf()).await;
    let provider = Arc::new(CapturingProvider::default());
    state.default_provider = Arc::clone(&provider) as Arc<dyn ModelProvider>;
    state.provider_catalogue =
        Arc::new(ProviderCatalogue::default().with_handle(
            "anthropic",
            "claude-compat-test",
            Arc::clone(&provider) as Arc<dyn ModelProvider>,
        ));
    let app = create_router(Arc::new(state)).await;
    (app, provider, temp)
}

/// The request's earlier turns, `system` and tool blocks must reach the
/// model: the endpoint replays them into a fresh session's log, and the
/// run projects them back as real history.
#[tokio::test]
async fn request_history_reaches_the_model_with_its_roles() {
    let (app, provider, _temp) = capturing_fixture().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "model": "claude-compat-test",
                        "max_tokens": 64,
                        "system": "be terse",
                        "messages": [
                            { "role": "user", "content": "one" },
                            { "role": "assistant", "content": [
                                { "type": "tool_use", "id": "toolu_1",
                                  "name": "bash", "input": { "cmd": "ls" } }
                            ]},
                            { "role": "user", "content": [
                                { "type": "tool_result",
                                  "tool_use_id": "toolu_1", "content": "a.txt" }
                            ]},
                            { "role": "user", "content": "three" },
                        ],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["content"][0]["text"], "captured");

    let requests = provider.requests();
    assert_eq!(requests.len(), 1, "one sampling pass ran");
    let messages = &requests[0];
    assert_eq!(
        messages[0].role,
        Role::System,
        "the agent's own system prompt still leads the request"
    );

    let text_of = |message: &Message| {
        message
            .content
            .iter()
            .filter_map(|part| part.text())
            .collect::<Vec<_>>()
            .join("")
    };
    let roles: Vec<Role> = messages.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::System,
            // The client's `system`, carried as a leading user turn
            // because the harness owns the real system message.
            Role::User,
            // ...then the request's own transcript...
            Role::User,
            Role::Assistant,
            Role::Tool,
            // ...then the final user turn, exactly once...
            Role::User,
            // ...and the harness's own runtime-context snapshot, which
            // the loop appends to every sampling pass.
            Role::User,
        ],
        "history roles must survive the round trip: {roles:?} from {messages:#?}"
    );
    assert!(
        text_of(&messages[1]).contains("be terse"),
        "the client's system text must reach the model: {:?}",
        text_of(&messages[1])
    );
    assert_eq!(text_of(&messages[2]), "one");
    match &messages[3].content {
        Content::Single(ContentPart::ToolUse(tool_use)) => {
            assert_eq!(tool_use.id, "toolu_1");
            assert_eq!(tool_use.name, "bash");
        }
        other => panic!("the assistant's tool call must survive: {other:?}"),
    }
    match &messages[4].content {
        Content::Single(ContentPart::ToolResult(result)) => {
            assert_eq!(result.tool_use_id, "toolu_1");
            assert_eq!(result.content[0].text(), Some("a.txt"));
        }
        other => panic!("the tool result must survive: {other:?}"),
    }
    assert_eq!(
        text_of(&messages[5]),
        "three",
        "the final user turn must appear once, as the current turn"
    );
    assert!(
        text_of(&messages[6]).starts_with("Current runtime context"),
        "the trailing message is the harness's snapshot, not a replayed turn"
    );
}

/// A `document` block reaches the model as a `ContentPart::Resource`,
/// on the turn being answered *and* on a replayed earlier one. Both
/// carry the shape the old `/api/v1/chat/*` wire's file attachment
/// produced — inline bytes as the historical `data:<media_type>;base64,…`
/// URI, a remote file as its URL — so an attachment sent through either
/// endpoint gives the agent the same part.
#[tokio::test]
async fn documents_reach_the_model_as_resources() {
    let (app, provider, _temp) = capturing_fixture().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "model": "claude-compat-test",
                        "max_tokens": 64,
                        "messages": [
                            { "role": "user", "content": [
                                { "type": "text", "text": "read the brief" },
                                { "type": "document", "source": {
                                    "type": "base64",
                                    "media_type": "application/pdf",
                                    "data": "JVBERi0=",
                                }},
                            ]},
                            { "role": "assistant", "content": "noted" },
                            { "role": "user", "content": [
                                { "type": "text", "text": "and this one" },
                                { "type": "document", "source": {
                                    "type": "url",
                                    "url": "https://example.test/spec.md",
                                }},
                            ]},
                        ],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["content"][0]["text"], "captured");

    let requests = provider.requests();
    assert_eq!(requests.len(), 1, "one sampling pass ran");
    let links: Vec<&ResourceLink> = requests[0]
        .iter()
        .flat_map(|message| message.content.iter())
        .filter_map(|part| match part {
            ContentPart::Resource(link) => Some(link),
            _ => None,
        })
        .collect();
    assert_eq!(
        links.len(),
        2,
        "the replayed document and the sent one must both arrive: {links:?}"
    );
    assert_eq!(
        links[0].uri, "data:application/pdf;base64,JVBERi0=",
        "the replayed document must keep the old wire's data-URI shape"
    );
    assert_eq!(links[0].mime_type.as_deref(), Some("application/pdf"));
    assert_eq!(
        links[1].uri, "https://example.test/spec.md",
        "a URL document reaches the model as the URL itself"
    );
}
