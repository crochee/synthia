//! Session binding and tool visibility on the Anthropic Messages
//! surface (`POST /v1/messages`).
//!
//! Two features turn on together, gated on the same condition — the
//! request carries `metadata.synthia_session_id`:
//!
//! - **Session binding.** The turn runs in the named session instead of
//!   a freshly minted `anthropic-*` one, and that session is neither
//!   re-seeded from the client's transcript nor closed when the turn
//!   ends, so the web client can address it for its sessions list,
//!   feedback, cancel and regenerate.
//! - **Tool blocks.** A call and its result become observable blocks
//!   the client can pair, which is what the web UI renders as
//!   `工具 · <name>` with 请求 / 结果 halves.
//!
//! The contract is documented on `routes::messages` and
//! `routes::messages::blocks`; this file pins the observable halves of
//! it. An external Anthropic client sends no such metadata and must see
//! neither feature — every test that pins an opted-in behaviour has a
//! sibling pinning the unopted-in one.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use synthia::{
    core::registry::Registry as _,
    harness::{AgentEntry, ReActAgent},
    provider::{
        CompletionRequest,
        CompletionResponse,
        Content,
        ContentPart,
        Message,
        ModelConfig,
        ModelProvider,
        ProviderConfig,
        TextContent,
        ToolUse,
    },
    test_support::FakeProvider,
};
use synthia_server::{
    create_router,
    state::{AppState, ProviderCatalogue},
};
use tower::ServiceExt;

/// The model id every request below names. `for_test` starts with an
/// empty catalogue, so each fixture registers this one.
const MODEL: &str = "claude-compat-test";

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    app: axum::Router,
    state: Arc<AppState>,
    /// Held so the on-disk session sinks outlive the router.
    _temp: tempfile::TempDir,
}

/// Build a router whose requests resolve `MODEL` to `provider`.
async fn make_fixture(provider: Arc<dyn ModelProvider>) -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let registry = synthia::session::manager::SessionRegistry::new(
        temp.path().join("sessions"),
    );
    let mut state =
        AppState::for_test(registry, temp.path().to_path_buf()).await;
    state.default_provider = Arc::clone(&provider);
    state.provider_catalogue =
        Arc::new(ProviderCatalogue::default().with_handle(
            "anthropic",
            MODEL,
            Arc::clone(&provider),
        ));
    let state = Arc::new(state);
    Fixture {
        app: create_router(Arc::clone(&state)).await,
        state,
        _temp: temp,
    }
}

/// A Messages body whose only user turn is `text`, optionally pinned to
/// `session_id` through the namespaced metadata key.
fn turn_body(session_id: Option<&str>, message: &str) -> String {
    body_with(session_id, message, false)
}

/// The same, asking for the protocol's SSE stream.
fn stream_body(session_id: Option<&str>, message: &str) -> String {
    body_with(session_id, message, true)
}

fn body_with(session_id: Option<&str>, message: &str, stream: bool) -> String {
    let mut body = json!({
        "model": MODEL,
        "max_tokens": 64,
        "messages": [{ "role": "user", "content": message }],
        "stream": stream,
    });
    if let Some(session_id) = session_id {
        body["metadata"] = json!({ "synthia_session_id": session_id });
    }
    body.to_string()
}

/// A body carrying a whole transcript — the shape a client that resends
/// its history sends. The final user turn is the one to answer; the
/// earlier turns are what `seed_history` replays.
fn transcript_body(session_id: &str, marker: &str) -> String {
    json!({
        "model": MODEL,
        "max_tokens": 64,
        "messages": [
            { "role": "user", "content": marker },
            { "role": "assistant", "content": "understood" },
            { "role": "user", "content": "and now?" },
        ],
        "metadata": { "synthia_session_id": session_id },
    })
    .to_string()
}

/// POST a raw body to `/v1/messages` and read the JSON reply.
async fn post(fix: &Fixture, body: &str) -> (StatusCode, Value) {
    let (status, bytes) = post_bytes(fix, body).await;
    let json = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "response body must be JSON ({error}): {:?}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, json)
}

async fn post_bytes(fix: &Fixture, body: &str) -> (StatusCode, Vec<u8>) {
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
    (status, bytes.to_vec())
}

/// GET a management path and read the JSON reply.
async fn get(fix: &Fixture, uri: &str) -> (StatusCode, Value) {
    let response = fix
        .app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
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

/// One SSE frame as it appears on the wire.
#[derive(Debug)]
struct Frame {
    event: String,
    data: Value,
}

fn parse_sse(bytes: &[u8]) -> Vec<Frame> {
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
        frames.push(Frame {
            event: event
                .unwrap_or_else(|| panic!("frame without `event:`: {block:?}")),
            data: data
                .unwrap_or_else(|| panic!("frame without `data:`: {block:?}")),
        });
    }
    frames
}

/// Every `content_block_start` frame's `content_block`, in order.
fn blocks(frames: &[Frame]) -> Vec<Value> {
    frames
        .iter()
        .filter(|frame| frame.event == "content_block_start")
        .map(|frame| frame.data["content_block"].clone())
        .collect()
}

/// One SSE body with its random message id removed — the only part of a
/// response that legitimately differs between two identical runs.
fn without_message_id(bytes: &[u8]) -> Vec<u8> {
    let text = String::from_utf8(bytes.to_vec()).expect("SSE is UTF-8");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("msg_") {
        out.push_str(&rest[..at]);
        out.push_str("msg_X");
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out.into_bytes()
}

// ---------------------------------------------------------------------------
// A provider that records what the model was asked
// ---------------------------------------------------------------------------

/// Answers `reply` to every sampling pass and records each request, so a
/// later turn's context can be inspected from the outside.
#[derive(Debug, Default)]
struct RecordingProvider {
    requests: parking_lot::Mutex<Vec<Vec<Message>>>,
}

impl RecordingProvider {
    fn requests(&self) -> Vec<Vec<Message>> {
        self.requests.lock().clone()
    }
}

#[async_trait::async_trait]
impl ModelProvider for RecordingProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), synthia::core::Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "recording"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: MODEL.to_string(),
            provider: "recording".to_string(),
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
        self.requests.lock().push(request.messages.as_ref().clone());
        Ok(CompletionResponse {
            content: Content::text("reply"),
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

/// Every text part of a recorded request, in order.
fn texts(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| message.content.extract_text())
        .collect()
}

// ---------------------------------------------------------------------------
// Session binding
// ---------------------------------------------------------------------------

/// A session id this server has never seen is *adopted*, and the next
/// turn continues it: both turns' prompts are in the one transcript, and
/// the second turn's model request carries the first turn's answer.
#[tokio::test]
async fn a_named_session_is_adopted_and_continued() {
    let provider = Arc::new(RecordingProvider::default());
    let fix =
        make_fixture(Arc::clone(&provider) as Arc<dyn ModelProvider>).await;

    let (status, body) =
        post(&fix, &turn_body(Some("chat-adopted"), "first")).await;
    assert_eq!(status, StatusCode::OK, "the first turn must run: {body}");
    let (status, body) =
        post(&fix, &turn_body(Some("chat-adopted"), "second")).await;
    assert_eq!(status, StatusCode::OK, "the second turn must run: {body}");

    // The second turn's model request carries the first turn — the
    // prompt in history, the answer as an assistant message. Without the
    // binding the two turns would be two sessions and this would be the
    // only message in the request.
    let requests = provider.requests();
    assert_eq!(requests.len(), 2, "one sampling pass per turn");
    let second = texts(&requests[1]).join("\n");
    assert!(
        second.contains("first"),
        "the second turn must see the first turn's prompt as history: \
         {second}"
    );
    assert!(
        second.contains("reply"),
        "the second turn must see the first turn's answer: {second}"
    );

    // ...and both prompts are in the one session's durable history,
    // which is what `GET /api/v1/sessions/{id}` renders.
    let (status, detail) = get(&fix, "/api/v1/sessions/chat-adopted").await;
    assert_eq!(status, StatusCode::OK, "detail: {detail}");
    let history = detail["history"].as_array().expect("history array");
    assert!(
        history.iter().any(|row| row.to_string().contains("first"))
            && history.iter().any(|row| row.to_string().contains("second")),
        "both turns must be in one transcript: {history:#?}"
    );
}

/// A malformed session id is refused rather than silently replaced: it
/// reaches the sink's filesystem path, where `../` is a traversal.
#[tokio::test]
async fn a_malformed_session_id_is_refused() {
    let provider = Arc::new(RecordingProvider::default());
    let fix =
        make_fixture(Arc::clone(&provider) as Arc<dyn ModelProvider>).await;

    for bad in ["../escape", "a/b", "..", "", "with space", "semi;colon"] {
        let (status, body) = post(&fix, &turn_body(Some(bad), "hi")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad:?}: {body}");
        assert_eq!(body["type"], "error", "{bad:?}: {body}");
        assert_eq!(
            body["error"]["type"], "invalid_request_error",
            "{bad:?}: {body}"
        );
        let message = body["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("synthia_session_id"),
            "the failure must name the field that is wrong: {message}"
        );
        assert!(
            message.contains(bad),
            "the failure must name the value that is wrong: {message}"
        );
        assert!(
            body.get("code").is_none(),
            "the protocol's envelope, never the application's: {body}"
        );
    }

    // Nothing escaped the sessions root, and no turn ran.
    let escaped = fix._temp.path().join("escape");
    assert!(
        !escaped.exists(),
        "a traversal-shaped id must not create anything outside the \
         sessions root"
    );
    assert!(
        provider.requests().is_empty(),
        "a refused binding must not reach the model"
    );
}

/// A pinned session is seeded once, not once per turn: the client may
/// resend its transcript every request and it must not be appended
/// twice.
#[tokio::test]
async fn a_pinned_session_is_not_re_seeded() {
    let provider = Arc::new(RecordingProvider::default());
    let fix =
        make_fixture(Arc::clone(&provider) as Arc<dyn ModelProvider>).await;

    // The same transcript, twice — a stateless client's view of the
    // conversation, which a stateless endpoint writes into the log on
    // every request.
    let transcript = transcript_body("chat-seeded", "SEED-MARKER");
    for turn in 0..2 {
        let (status, reply) = post(&fix, &transcript).await;
        assert_eq!(status, StatusCode::OK, "turn {turn}: {reply}");
    }

    let (status, detail) = get(&fix, "/api/v1/sessions/chat-seeded").await;
    assert_eq!(status, StatusCode::OK, "detail: {detail}");
    let history = detail["history"].as_array().expect("history array");
    let seeded = history
        .iter()
        .filter(|row| {
            row["type"] == "Message" && row.to_string().contains("SEED-MARKER")
        })
        .count();
    assert_eq!(
        seeded, 1,
        "the client's transcript is seeded on the session's first turn \
         only; a second seed would duplicate every earlier turn and grow \
         the log quadratically. history={history:#?}"
    );
}

/// A pinned session stays live after its turn: not closed, not evicted.
/// An ephemeral one is both — which is why the two cannot share a code
/// path.
#[tokio::test]
async fn a_pinned_session_survives_its_turn() {
    let provider = Arc::new(RecordingProvider::default());
    let fix =
        make_fixture(Arc::clone(&provider) as Arc<dyn ModelProvider>).await;

    let (status, body) = post(&fix, &turn_body(Some("chat-live"), "hi")).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The cached controller is the *same* live one — not a closed one,
    // and not one recreated on the next lookup. This is what the
    // sessions list/detail page, feedback, cancel and regenerate all
    // depend on between turns.
    let cached = |fix: &Fixture| {
        fix.state
            .active_sessions
            .get(&("dev".to_string(), "chat-live".to_string()))
            .map(|entry| Arc::clone(entry.value()))
    };
    let before = cached(&fix).expect("the pinned controller must be cached");
    assert!(
        before.is_alive(),
        "a pinned session's controller must not be closed at end of turn"
    );

    // A management call between turns reaches that same controller:
    // cancel resolves through `get_or_create_session_controller`, which
    // must hand back the cached one rather than closing a second.
    let response = fix
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/chat/sessions/chat-live/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "cancel must find the pinned session's live controller"
    );
    let after = cached(&fix).expect("cancel must not evict the session");
    assert!(
        Arc::ptr_eq(&before, &after),
        "cancel must address the live controller, not a replacement"
    );

    // And both management surfaces still serve it.
    let (status, detail) = get(&fix, "/api/v1/sessions/chat-live").await;
    assert_eq!(status, StatusCode::OK, "detail: {detail}");
    assert!(
        detail["history"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "the transcript must be readable after the turn: {detail}"
    );
    let (status, events) = get(&fix, "/api/v1/sessions/chat-live/events").await;
    assert_eq!(status, StatusCode::OK, "events: {events}");

    // A second turn still lands in it, on the same controller.
    let (status, body) =
        post(&fix, &turn_body(Some("chat-live"), "again")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        Arc::ptr_eq(
            &before,
            &cached(&fix).expect("the session stays cached across turns")
        ),
        "a second turn must reuse the pinned controller"
    );

    // An unbound request is still ephemeral: its session is evicted when
    // its turn ends, so a stateless client cannot grow the cache.
    let (status, body) = post(&fix, &turn_body(None, "hi")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let live: Vec<String> = fix
        .state
        .active_sessions
        .iter()
        .map(|entry| entry.key().1.clone())
        .collect();
    assert!(
        !live.iter().any(|id| id.starts_with("anthropic-")),
        "a minted session must still be evicted at end of turn: {live:?}"
    );
    assert!(
        live.contains(&"chat-live".to_string()),
        "a later unbound request must not disturb a pinned session: \
         {live:?}"
    );
}

// ---------------------------------------------------------------------------
// Agent selection
// ---------------------------------------------------------------------------

/// The client's agent choice is a dispatch input: the named agent's own
/// instructions reach the model.
#[tokio::test]
async fn a_named_agent_answers_the_turn() {
    let provider = Arc::new(RecordingProvider::default());
    let fix =
        make_fixture(Arc::clone(&provider) as Arc<dyn ModelProvider>).await;

    // A second agent whose descriptor is identifiable in the system
    // prompt the run assembles.
    let researcher = ReActAgent::new(
        Arc::clone(&provider) as Arc<dyn ModelProvider>,
        Arc::new(synthia::tool::ToolRegistry::new()),
    )
    .with_name("researcher")
    .with_instructions("SYNTHIA-RESEARCHER-MARKER");
    fix.state
        .agent_registry
        .put(AgentEntry::new(Arc::new(researcher)))
        .await
        .expect("the researcher registers");

    let (status, body) = post(
        &fix,
        &json!({
            "model": MODEL,
            "max_tokens": 64,
            "messages": [{ "role": "user", "content": "hi" }],
            "metadata": {
                "synthia_session_id": "chat-agent",
                "synthia_agent_name": "researcher",
            },
        })
        .to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let requested = texts(&provider.requests()[0]).join("\n");
    assert!(
        requested.contains("SYNTHIA-RESEARCHER-MARKER"),
        "`metadata.synthia_agent_name` must select the agent, not just \
         label it: {requested}"
    );
}

// ---------------------------------------------------------------------------
// Tool visibility
// ---------------------------------------------------------------------------

/// A `read` call the run executes, then a text answer: a two-iteration
/// run whose first pass asks for a tool and whose second answers.
fn tool_run_provider() -> Arc<dyn ModelProvider> {
    let call = CompletionResponse {
        content: Content::Multi(vec![
            text_part("working"),
            ContentPart::ToolUse(ToolUse {
                id: "toolu_read_1".to_string(),
                name: "read".to_string(),
                input: json!({ "file_path": "note.txt" }),
            }),
        ]),
        ..CompletionResponse::default()
    };
    let answer = CompletionResponse {
        content: Content::text("done"),
        ..CompletionResponse::default()
    };
    Arc::new(FakeProvider::new(vec![call, answer]))
}

fn text_part(text: &str) -> ContentPart {
    ContentPart::Text(TextContent {
        text: text.to_string(),
        cache_control: None,
    })
}

/// Write the file the tool run reads, and return the fixture.
async fn tool_fixture() -> Fixture {
    let fix = make_fixture(tool_run_provider()).await;
    std::fs::write(fix._temp.path().join("note.txt"), "note contents\n")
        .expect("the workspace file is writable");
    fix
}

/// An opted-in request sees the call and its result as separate blocks
/// paired by the call id — the two halves the web UI renders.
#[tokio::test]
async fn an_opted_in_stream_carries_the_call_and_its_result() {
    let fix = tool_fixture().await;
    let (status, bytes) =
        post_bytes(&fix, &stream_body(Some("chat-tools"), "read it")).await;
    assert_eq!(status, StatusCode::OK);
    let frames = parse_sse(&bytes);

    let blocks = blocks(&frames);
    let kinds: Vec<&str> = blocks
        .iter()
        .map(|block| block["type"].as_str().unwrap_or("?"))
        .collect();
    assert_eq!(
        kinds,
        vec!["text", "tool_use", "tool_result", "text"],
        "the call and its result are blocks of their own: {frames:#?}"
    );

    let call = &blocks[1];
    assert_eq!(call["id"], "toolu_read_1");
    assert_eq!(call["name"], "read");
    assert_eq!(
        call["input"],
        json!({}),
        "a streamed tool_use opens with an empty input and grows by delta"
    );
    let input: String = frames
        .iter()
        .filter(|frame| frame.data["delta"]["type"] == "input_json_delta")
        .map(|frame| {
            frame.data["delta"]["partial_json"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&input).unwrap(),
        json!({ "file_path": "note.txt" }),
        "the call's arguments ride the input_json_delta"
    );

    let result = &blocks[2];
    assert_eq!(
        result["tool_use_id"], "toolu_read_1",
        "the result names the call it answers"
    );
    assert_eq!(result["name"], "read");
    assert_eq!(result["is_error"], false);
    assert!(
        result["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("note contents")),
        "the tool's output must reach the client: {result}"
    );

    // A result block carries no deltas: `content_block_start` is the
    // whole thing, and only its `content_block_stop` follows.
    let at = frames
        .iter()
        .position(|frame| {
            frame.event == "content_block_start"
                && frame.data["content_block"]["type"] == "tool_result"
        })
        .expect("a result block");
    let stop = frames.get(at + 1).expect("the result block's stop");
    assert_eq!(
        stop.event, "content_block_stop",
        "a result arrives whole, so nothing may follow its start: {stop:?}"
    );
    assert_eq!(
        stop.data["index"], frames[at].data["index"],
        "the stop must close the block its start opened"
    );
}

/// A failing tool is distinguishable from a succeeding one without
/// reading its output.
#[tokio::test]
async fn an_opted_in_result_flags_failure() {
    // A tool the registry does not have: the run produces an error
    // result for it rather than a silent no-op.
    let call = CompletionResponse {
        content: Content::Single(ContentPart::ToolUse(ToolUse {
            id: "toolu_missing_1".to_string(),
            name: "no_such_tool".to_string(),
            input: json!({}),
        })),
        ..CompletionResponse::default()
    };
    let answer = CompletionResponse {
        content: Content::text("done"),
        ..CompletionResponse::default()
    };
    let fix =
        make_fixture(Arc::new(FakeProvider::new(vec![call, answer]))).await;

    let (status, bytes) =
        post_bytes(&fix, &stream_body(Some("chat-fail"), "go")).await;
    assert_eq!(status, StatusCode::OK);
    let blocks = blocks(&parse_sse(&bytes));
    let result = blocks
        .iter()
        .find(|block| block["type"] == "tool_result")
        .expect("the failed call's result must be observable")
        .clone();
    assert_eq!(
        result["is_error"], true,
        "a failure must be visible as one: {result}"
    );
    assert_eq!(result["tool_use_id"], "toolu_missing_1");
}

/// An external Anthropic client's bytes are unchanged by the tool
/// surface: the call it never asked for and the result it never awaited
/// produce nothing at all.
#[tokio::test]
async fn an_unopted_in_stream_is_unchanged() {
    let fix = tool_fixture().await;
    let (status, bytes) = post_bytes(&fix, &stream_body(None, "read it")).await;
    assert_eq!(status, StatusCode::OK);

    let frames = parse_sse(&bytes);
    assert_eq!(
        blocks(&frames)
            .iter()
            .map(|block| block["type"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>(),
        vec!["text"],
        "an external client sees the run's text and nothing else: {frames:#?}"
    );
    let raw = String::from_utf8(without_message_id(&bytes)).unwrap();
    assert!(
        !raw.contains("tool_use") && !raw.contains("tool_result"),
        "no tool traffic may leak to an external client: {raw}"
    );

    // ...and the whole stream is exactly what this run produced before
    // the opt-in existed: every frame's name and complete payload (the
    // message id is the one random part, normalised). An extra field, an
    // extra frame or a reordered block fails here. The run's text is one
    // block — the call and the result contribute nothing, so nothing
    // closes the block between the two sampling passes.
    let expected: Vec<(&str, Value)> = vec![
        (
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": frames[0].data["message"]["id"].clone(),
                    "type": "message",
                    "role": "assistant",
                    "model": MODEL,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": { "input_tokens": 0, "output_tokens": 0 },
                },
            }),
        ),
        (
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" },
            }),
        ),
        (
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "working" },
            }),
        ),
        (
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "done" },
            }),
        ),
        (
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 }),
        ),
        (
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                },
                "usage": { "input_tokens": 0, "output_tokens": 0 },
            }),
        ),
        ("message_stop", json!({ "type": "message_stop" })),
    ];
    assert!(
        frames[0].data["message"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("msg_")),
        "a message id must keep the protocol's prefix: {frames:#?}"
    );

    let actual: Vec<(&str, Value)> = frames
        .iter()
        .map(|frame| (frame.event.as_str(), frame.data.clone()))
        .collect();
    assert_eq!(
        actual, expected,
        "an external client's stream must be frame-for-frame what it was"
    );
}
