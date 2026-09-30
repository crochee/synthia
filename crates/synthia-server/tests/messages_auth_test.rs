//! Auth tests for the Anthropic Messages surface.
//!
//! The whole file is one test on purpose: `AuthMiddleware` captures
//! `SYNTHIA_API_KEY` once, at construction, so a test that wants a
//! *configured* key must set the variable before building its router.
//! One test keeps that process-wide mutation single-shot and
//! deterministic, instead of racing sibling tests inside this binary.
//!
//! The contract being pinned: the compatibility surface is mounted on
//! the protected router and accepts the protocol's `x-api-key` header
//! as a credential — it is never unauthenticated by accident, and the
//! existing `Authorization: Bearer` path is unchanged.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use synthia::{provider::ModelProvider, test_support::FakeProvider};
use synthia_server::{
    create_router,
    state::{AppState, ProviderCatalogue},
};
use tower::ServiceExt;

/// The key this process configures; every router built here sees it.
const KEY: &str = "anthropic-compat-test-key";

struct Fixture {
    app: axum::Router,
    _temp: tempfile::TempDir,
}

async fn make_fixture() -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let registry = synthia::session::manager::SessionRegistry::new(
        temp.path().join("sessions"),
    );
    let mut state =
        AppState::for_test(registry, temp.path().to_path_buf()).await;
    // A deterministic answer, so the positive cases below prove a real
    // run reached the provider rather than merely passing the gate.
    // Two canned answers because this test drives two authorised runs —
    // `FakeProvider` errors once its script is exhausted on purpose.
    let answer = || synthia::provider::CompletionResponse {
        content: synthia::provider::Content::text("authorized"),
        ..synthia::provider::CompletionResponse::default()
    };
    let provider: Arc<dyn ModelProvider> =
        Arc::new(FakeProvider::new(vec![answer(), answer()]));
    state.default_provider = Arc::clone(&provider);
    // The request's `model` resolves against the catalogue, and a value
    // no configured provider advertises is refused — so the fixture
    // registers the one the body posts.
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

/// POST a minimal Messages body with the given credential headers.
async fn post_with(
    fix: &Fixture,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let body = json!({
        "model": "claude-compat-test",
        "max_tokens": 16,
        "messages": [{ "role": "user", "content": "hi" }],
    })
    .to_string();
    let response = fix
        .app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
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

#[tokio::test]
async fn x_api_key_gates_the_endpoint_when_a_key_is_configured() {
    // SAFETY: see the module docs — one test, set before any router in
    // this process is constructed, never removed.
    unsafe {
        std::env::set_var("SYNTHIA_API_KEY", KEY);
    }
    let fix = make_fixture().await;

    // 1. The Anthropic protocol's credential authenticates.
    let (status, body) = post_with(&fix, &[("x-api-key", KEY)]).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a valid x-api-key must authenticate: {body}"
    );
    assert_eq!(body["type"], "message");

    // 2. A wrong Anthropic credential does not.
    let (status, _) = post_with(&fix, &[("x-api-key", "not-the-key")]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 3. No credential at all does not either, and the rejection
    //    carries the protocol's envelope so an SDK can parse it.
    let (status, body) = post_with(&fix, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["type"], "error");
    assert_eq!(body["error"]["type"], "authentication_error");
    assert!(
        body.get("code").is_none(),
        "the app envelope must not leak on this path: {body}"
    );

    // 4. The server's pre-existing Bearer path still works.
    let (status, body) =
        post_with(&fix, &[("authorization", &format!("Bearer {KEY}"))]).await;
    assert_eq!(status, StatusCode::OK, "Bearer must still authenticate");
    assert_eq!(body["type"], "message");

    // 5. `Authorization` is the credential when both headers are
    //    present, so a wrong one is not rescued by the other.
    let (status, _) = post_with(
        &fix,
        &[("authorization", "Bearer wrong"), ("x-api-key", KEY)],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
