//! The model selector, end to end: a turn's `model` field must pick
//! the provider that samples it.
//!
//! `SendMessageRequest.model` was parsed and never read, so a
//! selection named `anthropic/…` ran the configured default and the
//! user's choice was a silent no-op. These tests pin the fix at the
//! only observable that matters — *which provider handled the turn* —
//! by giving each catalogue entry its own `FakeProvider` and reading
//! its `call_count`.
//!
//! They also pin the two spellings the two wire surfaces send:
//! `"<provider>/<model>"` (what `/api/v1/models` advertises and the
//! chat selector posts) and a bare `"<model>"` (what an
//! Anthropic-protocol client sends in `model`), plus the refusal that
//! makes a mistyped selection visible instead of silently running the
//! default.
//!
//! Hermetic: every provider is a fake, so no run touches the network.

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use synthia::{
    provider::traits::ModelProvider,
    session::manager::SessionRegistry,
    test_support::FakeProvider,
};
use synthia_server::{
    create_router,
    state::{AppState, ProviderCatalogue},
};
use tower::ServiceExt;

/// How long a turn may take to reach its provider before a test calls
/// it a failure. Generous: the run is a spawned task on the shared
/// runtime, not a synchronous call.
const TURN_DEADLINE: Duration = Duration::from_secs(10);

const OPENAI_MODEL: &str = "gpt-4o";
const ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
/// The deployment *default* provider, which no selection in this file
/// names. Keeping it distinct is what lets an assertion tell "the
/// selection resolved" from "the turn fell back to the default" — the
/// exact defect these tests pin.
const DEFAULT_MODEL: &str = "llama-3.3-70b";

struct Fixture {
    app: axum::Router,
    openai: Arc<FakeProvider>,
    anthropic: Arc<FakeProvider>,
    default: Arc<FakeProvider>,
    _temp: tempfile::TempDir,
}

impl Fixture {
    /// `(default, openai, anthropic)` call counts. A turn runs on
    /// exactly one provider, so exactly one field moves.
    fn calls(&self) -> (usize, usize, usize) {
        let load = |provider: &Arc<FakeProvider>| {
            provider
                .call_count
                .load(std::sync::atomic::Ordering::SeqCst)
        };
        (
            load(&self.default),
            load(&self.openai),
            load(&self.anthropic),
        )
    }

    /// Wait until some provider has sampled a turn, or fail.
    async fn wait_for_a_turn(&self) {
        let deadline = tokio::time::Instant::now() + TURN_DEADLINE;
        while tokio::time::Instant::now() < deadline {
            let (default, openai, anthropic) = self.calls();
            if default + openai + anthropic > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no provider sampled the turn within {TURN_DEADLINE:?}");
    }

    /// Wait until `provider` has sampled a turn, or fail.
    async fn wait_for(&self, count: fn(&Self) -> usize) {
        let deadline = tokio::time::Instant::now() + TURN_DEADLINE;
        while tokio::time::Instant::now() < deadline {
            if count(self) > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the expected provider never sampled within {TURN_DEADLINE:?}");
    }

    fn anthropic_calls(&self) -> usize {
        self.calls().2
    }

    fn default_calls(&self) -> usize {
        self.calls().0
    }
}

/// Build a state whose catalogue offers three providers that advertise
/// different models, and whose default is the one no test selects.
async fn make_fixture() -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let sessions_root = temp.path().to_path_buf();
    let registry = SessionRegistry::new(sessions_root.clone());
    let mut state = AppState::for_test(registry, sessions_root).await;

    let openai = Arc::new(FakeProvider::text("openai answer"));
    let anthropic = Arc::new(FakeProvider::text("anthropic answer"));
    let default = Arc::new(FakeProvider::text("default answer"));
    let default_provider: Arc<dyn ModelProvider> = default.clone();
    let openai_handle: Arc<dyn ModelProvider> = openai.clone();
    let anthropic_handle: Arc<dyn ModelProvider> = anthropic.clone();
    let default_handle: Arc<dyn ModelProvider> = default.clone();
    state.default_provider = default_provider;
    state.provider_catalogue = Arc::new(
        ProviderCatalogue::default()
            .with_handle("local", DEFAULT_MODEL, default_handle)
            .with_handle("openai", OPENAI_MODEL, openai_handle)
            .with_handle("anthropic", ANTHROPIC_MODEL, anthropic_handle),
    );

    Fixture {
        app: create_router(Arc::new(state)).await,
        openai,
        anthropic,
        default,
        _temp: temp,
    }
}

async fn send(
    app: &axum::Router,
    uri: &str,
    method: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder().uri(uri).method(method);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Create the session, then queue one turn against it.
async fn turn(
    app: &axum::Router,
    session_id: &str,
    model: Option<&str>,
) -> (StatusCode, Value) {
    let (status, body) = send(
        app,
        "/api/v1/chat/sessions",
        "POST",
        Some(json!({ "session_id": session_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create_session: {body}");

    let mut request = json!({ "text": "hello" });
    if let Some(model) = model {
        request["model"] = json!(model);
    }
    send(
        app,
        &format!("/api/v1/chat/sessions/{session_id}/messages"),
        "POST",
        Some(request),
    )
    .await
}

/// A qualified selection runs the named provider and only that one.
#[tokio::test]
async fn qualified_model_selection_runs_the_named_provider() {
    let fixture = make_fixture().await;

    let (status, body) = turn(
        &fixture.app,
        "selector-qualified",
        Some(&format!("openai/{OPENAI_MODEL}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "send_message: {body}");
    fixture.wait_for_a_turn().await;
    assert_eq!(
        fixture.calls(),
        (0, 1, 0),
        "`openai/{OPENAI_MODEL}` must sample the OpenAI provider"
    );

    let (status, body) = turn(
        &fixture.app,
        "selector-qualified-anthropic",
        Some(&format!("anthropic/{ANTHROPIC_MODEL}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "send_message: {body}");
    fixture.wait_for(Fixture::anthropic_calls).await;
    assert_eq!(
        fixture.calls(),
        (0, 1, 1),
        "`anthropic/{ANTHROPIC_MODEL}` must sample the Anthropic provider"
    );
}

/// The Anthropic wire sends a bare model id in `model`, so a selection
/// that matches exactly one provider's advertised model resolves too.
#[tokio::test]
async fn bare_model_selection_runs_the_provider_offering_it() {
    let fixture = make_fixture().await;

    let (status, body) =
        turn(&fixture.app, "selector-bare", Some(ANTHROPIC_MODEL)).await;
    assert_eq!(status, StatusCode::OK, "send_message: {body}");
    fixture.wait_for_a_turn().await;
    assert_eq!(
        fixture.calls(),
        (0, 0, 1),
        "the bare `{ANTHROPIC_MODEL}` must resolve to its one provider"
    );
}

/// Regression guard for the common case: a turn that names no model
/// still runs the deployment default.
#[tokio::test]
async fn turn_without_a_selection_runs_the_default_provider() {
    let fixture = make_fixture().await;

    let (status, body) = turn(&fixture.app, "selector-default", None).await;
    assert_eq!(status, StatusCode::OK, "send_message: {body}");
    fixture.wait_for_a_turn().await;
    assert_eq!(
        fixture.calls(),
        (1, 0, 0),
        "an absent `model` field must keep the default provider"
    );
}

/// A selection the deployment cannot honour is the caller's error, and
/// the message has to name the value — silently running the default is
/// the defect this whole surface exists to remove.
#[tokio::test]
async fn unresolvable_model_selection_is_rejected_and_names_the_value() {
    let fixture = make_fixture().await;

    for selection in ["gemini/gemini-2.0-pro", "openai/gpt-5", "no-such-model"]
    {
        let (status, body) = turn(
            &fixture.app,
            &format!("selector-bad-{}", selection.replace(['/', '.'], "-")),
            Some(selection),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "`{selection}` must be rejected, got {body}"
        );
        let message = body["message"].as_str().unwrap_or_default();
        assert!(
            message.contains(selection),
            "the error must name `{selection}`: {message}"
        );
    }

    assert_eq!(
        fixture.calls(),
        (0, 0, 0),
        "a rejected selection must not run any provider"
    );
}

/// The selection belongs to one turn, not to the session.
///
/// A sticky swap would pass every test above while quietly changing the
/// meaning of the *next* turn — a user who picked a model once would
/// keep sampling it after switching back, and two operations queued
/// behind a running turn could swap each other's model out.
#[tokio::test]
async fn a_selection_does_not_stick_to_the_next_turn() {
    let fixture = make_fixture().await;

    let (status, body) = turn(
        &fixture.app,
        "selector-not-sticky",
        Some(&format!("anthropic/{ANTHROPIC_MODEL}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "first send_message: {body}");
    fixture.wait_for(Fixture::anthropic_calls).await;
    assert_eq!(fixture.calls(), (0, 0, 1));

    // The same session, this time naming no model.
    let (status, body) = send(
        &fixture.app,
        "/api/v1/chat/sessions/selector-not-sticky/messages",
        "POST",
        Some(json!({ "text": "again" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "second send_message: {body}");
    fixture.wait_for(Fixture::default_calls).await;
    assert_eq!(
        fixture.calls(),
        (1, 0, 1),
        "the second turn must fall back to the default provider"
    );
}

/// A fake that reports a caller-chosen provider and model name.
///
/// The durable `request_header` row records *names*, so two
/// `FakeProvider`s (which both call themselves `fake`) cannot tell a
/// mislabelled header from a correct one.
struct LabelledProvider {
    label: String,
    model: String,
    inner: FakeProvider,
}

#[async_trait::async_trait]
impl ModelProvider for LabelledProvider {
    async fn initialize(
        &mut self,
        _config: synthia::provider::ProviderConfig,
    ) -> Result<(), synthia::core::Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        &self.label
    }

    fn model_config(&self) -> synthia::provider::ModelConfig {
        synthia::provider::ModelConfig {
            name: self.model.clone(),
            provider: self.label.clone(),
            context_window: 128_000,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        request: synthia::provider::CompletionRequest,
    ) -> Result<synthia::provider::CompletionResponse, synthia::core::Error>
    {
        self.inner.complete(request).await
    }
}

/// The run's durable epoch marker names the provider that actually
/// sampled the turn.
///
/// The header used to be read off the session's configured provider, so
/// a selected turn was recorded under the deployment default — the log
/// claimed a model ran that never did, and every replay/analytics
/// consumer reading it inherited the lie.
#[tokio::test]
async fn the_durable_request_header_names_the_provider_that_ran() {
    let temp = tempfile::TempDir::new().unwrap();
    let sessions_root = temp.path().to_path_buf();
    let registry = SessionRegistry::new(sessions_root.clone());
    let mut state = AppState::for_test(registry, sessions_root).await;

    let selected: Arc<dyn ModelProvider> = Arc::new(LabelledProvider {
        label: "openai-provider".to_string(),
        model: OPENAI_MODEL.to_string(),
        inner: FakeProvider::text("selected answer"),
    });
    let default: Arc<dyn ModelProvider> = Arc::new(LabelledProvider {
        label: "default-provider".to_string(),
        model: DEFAULT_MODEL.to_string(),
        inner: FakeProvider::text("default answer"),
    });
    state.default_provider = Arc::clone(&default);
    state.provider_catalogue = Arc::new(
        ProviderCatalogue::default()
            .with_handle("local", DEFAULT_MODEL, default)
            .with_handle("openai", OPENAI_MODEL, selected),
    );
    let state = Arc::new(state);
    let app = create_router(Arc::clone(&state)).await;

    let session_id = "selector-header";
    let (status, body) =
        turn(&app, session_id, Some(&format!("openai/{OPENAI_MODEL}"))).await;
    assert_eq!(status, StatusCode::OK, "send_message: {body}");

    let sink = state.session_manager.sink(
        synthia::session::manager::SERVER_DEFAULT_USER_ID,
        session_id,
    );
    let deadline = tokio::time::Instant::now() + TURN_DEADLINE;
    let header = loop {
        let rows = sink.read().await.unwrap_or_default();
        if let Some(row) = rows.iter().find(|row| {
            row.get("type").and_then(Value::as_str) == Some("request_header")
        }) {
            break row.clone();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no request_header row landed within {TURN_DEADLINE:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };

    assert_eq!(
        header["data"]["provider"], "openai-provider",
        "the header must name the provider that ran: {header}"
    );
    assert_eq!(
        header["data"]["model"], OPENAI_MODEL,
        "the header must name the model that ran: {header}"
    );
}
