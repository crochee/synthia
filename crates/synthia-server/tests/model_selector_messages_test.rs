//! The model selector on the Anthropic wire.
//!
//! The web UI sends through `POST /v1/messages`, so per-turn provider
//! selection has to hold there too — and the protocol's `model` field
//! is a *bare* model name, not the `"<provider>/<model>"` string the
//! chat surface posts. `AppState::resolve_provider` accepts both
//! spellings; these tests pin the Anthropic path against the same
//! observable the chat-path tests use: which provider sampled the
//! turn, read off each catalogue entry's `FakeProvider`.
//!
//! The refusal matters as much as the selection here: the protocol has
//! a defined envelope for a client error, so an unusable `model` must
//! answer with one and name the value — never quietly answer from
//! another provider.
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

/// How long the run behind a turn may take to reach its provider.
const TURN_DEADLINE: Duration = Duration::from_secs(10);

const OPENAI_MODEL: &str = "gpt-4o";
const ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
/// A third entry that is the deployment *default* and is never the
/// provider a selection names. Every assertion below can therefore tell
/// "the selection resolved" from "the turn fell back to the default",
/// which is exactly the defect these tests exist to catch.
const DEFAULT_MODEL: &str = "llama-3.3-70b";

struct Fixture {
    app: axum::Router,
    openai: Arc<FakeProvider>,
    anthropic: Arc<FakeProvider>,
    default: Arc<FakeProvider>,
    _temp: tempfile::TempDir,
}

impl Fixture {
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

    /// `POST /v1/messages` with one user turn, returning the status and
    /// the parsed body.
    async fn message(&self, model: &str) -> (StatusCode, Value) {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "model": model,
                            "max_tokens": 64,
                            "messages": [{ "role": "user", "content": "hi" }],
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

/// A state whose catalogue offers three providers, defaulting to the
/// one no test selects.
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

/// A qualified `model` runs the provider it names.
#[tokio::test]
async fn qualified_model_runs_the_named_provider() {
    let fixture = make_fixture().await;

    let (status, body) = fixture
        .message(&format!("anthropic/{ANTHROPIC_MODEL}"))
        .await;
    assert_eq!(status, StatusCode::OK, "response: {body}");
    fixture.wait_for_a_turn().await;
    assert_eq!(
        fixture.calls(),
        (0, 0, 1),
        "`anthropic/{ANTHROPIC_MODEL}` must sample the Anthropic provider"
    );
}

/// The protocol sends a bare model id, so the same field selects a
/// provider by the model it advertises.
#[tokio::test]
async fn bare_model_runs_the_provider_offering_it() {
    let fixture = make_fixture().await;

    let (status, body) = fixture.message(OPENAI_MODEL).await;
    assert_eq!(status, StatusCode::OK, "response: {body}");
    fixture.wait_for_a_turn().await;
    assert_eq!(
        fixture.calls(),
        (0, 1, 0),
        "the bare `{OPENAI_MODEL}` must resolve to the provider offering it"
    );
}

/// An unusable `model` answers with the protocol's error envelope,
/// names the value, and runs nothing.
#[tokio::test]
async fn unresolvable_model_answers_the_protocol_error_envelope() {
    let fixture = make_fixture().await;

    let (status, body) = fixture.message("gemini/gemini-2.0-pro").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {body}");
    assert_eq!(
        body["type"], "error",
        "the Anthropic wire answers with its own envelope: {body}"
    );
    assert_eq!(body["error"]["type"], "invalid_request_error");
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("gemini/gemini-2.0-pro"),
        "the error must name the value: {message}"
    );
    assert_eq!(
        fixture.calls(),
        (0, 0, 0),
        "a rejected model must not run any provider"
    );
}
