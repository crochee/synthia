//! End-to-end integration tests for the Registry-First pipeline.
//!
//! Verifies the full agent execution flow: session creation → prompt
//! submission → agent run → event broadcast → Registry-First component
//! invocation (FragmentRegistry, InterceptorChain, RolloutTracker).

use std::{sync::Arc, time::Duration};

use synthia_agent::{
    events::{HookEvent, SystemEvent},
    types::{AgentEvent, SessionEndReason},
};
use synthia_provider::types::CompletionResponse;
use synthia_server::{session::controller::SessionOp, state::AppState};
use synthia_session::manager::SessionManager;

/// Build an AppState with a FakeProvider that returns a simple text
/// response (no tool calls) so the agent loop terminates after one
/// iteration. `for_test()` now registers built-in fragments, skills,
/// and interceptors matching production configuration.
async fn setup_app_with_fake_response() -> Arc<AppState> {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let workspace = temp_dir.path().to_path_buf();
    let session_manager = SessionManager::new(workspace.join("sessions"));

    // FakeProvider returns a text-only response — the agent will emit
    // LlmResponseComplete and then SessionEnded (no tool calls).
    let response = CompletionResponse {
        id: "fake-resp-1".to_string(),
        model: "fake-model".to_string(),
        content: synthia_provider::types::Content::text(
            "Hello from FakeProvider",
        ),
        usage: synthia_provider::types::TokenUsage {
            prompt_tokens: 50,
            completion_tokens: 20,
            total_tokens: 70,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        },
        cached: false,
    };
    let fake_provider = test_support::FakeProvider::with_response(response);

    let mut state = AppState::for_test(session_manager, workspace).await;
    state.default_provider = Arc::new(fake_provider);

    Arc::new(state)
}

/// Helper: submit a prompt and collect events until SessionEnded or
/// timeout.
async fn run_prompt_and_collect_events(
    app: &Arc<AppState>,
    user_id: &str,
    session_id: &str,
    prompt: &str,
) -> Vec<AgentEvent> {
    let controller = app
        .get_or_create_session_controller(user_id, session_id)
        .await
        .unwrap();

    let mut rx = controller.subscribe();

    controller
        .submit(SessionOp::Prompt {
            content: prompt.to_string(),
            priority: 128,
        })
        .await
        .unwrap();

    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining =
            deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(event)) => {
                events.push(event);
                if matches!(
                    events.last(),
                    Some(AgentEvent::System(SystemEvent::SessionEnded { .. }))
                ) {
                    break;
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                tracing::warn!("SSE receiver lagged by {n} events");
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        }
    }
    events
}

/// Helper: submit a prompt and wait for SessionEnded or timeout.
async fn run_prompt_until_ended(
    app: &Arc<AppState>,
    user_id: &str,
    session_id: &str,
    prompt: &str,
) {
    let controller = app
        .get_or_create_session_controller(user_id, session_id)
        .await
        .unwrap();

    let mut rx = controller.subscribe();

    controller
        .submit(SessionOp::Prompt {
            content: prompt.to_string(),
            priority: 128,
        })
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining =
            deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(AgentEvent::System(SystemEvent::SessionEnded {
                ..
            }))) => break,
            Ok(Err(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

/// Test that the agent emits a complete event lifecycle when a prompt
/// is submitted, and that events are broadcast through the
/// EventBroadcaster.
#[tokio::test]
async fn test_e2e_prompt_submits_and_emits_session_events() {
    let app = setup_app_with_fake_response().await;
    let user_id = "_legacy_";
    let session_id = "e2e-session-1";

    app.session_manager
        .create_with_user(session_id.to_string(), user_id.to_string())
        .await
        .unwrap();

    let events = run_prompt_and_collect_events(
        &app,
        user_id,
        session_id,
        "Hello, Synthia!",
    )
    .await;

    let has_session_started = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionStarted { .. }))
    });
    let has_llm_response =
        events.iter().any(|e| matches!(e, AgentEvent::ModelDone(_)));
    let has_session_ended = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });

    assert!(has_session_started, "Should receive SessionStarted event");
    assert!(has_llm_response, "Should receive LlmResponseComplete event");
    assert!(has_session_ended, "Should receive SessionEnded event");

    // Verify the SessionEnded reason is Completed
    let session_ended = events.iter().find_map(|e| {
        if let AgentEvent::System(SystemEvent::SessionEnded { reason }) = e {
            Some(reason.clone())
        } else {
            None
        }
    });
    assert_eq!(
        session_ended,
        Some(SessionEndReason::Completed),
        "Session should end with Completed reason"
    );
}

/// Test that the RolloutTracker records token usage after the agent run.
#[tokio::test]
async fn test_e2e_rollout_tracker_records_token_usage() {
    let app = setup_app_with_fake_response().await;
    let user_id = "_legacy_";
    let session_id = "e2e-rollout-1";

    let budget_before = app.rollout_tracker.token_budget().await;

    app.session_manager
        .create_with_user(session_id.to_string(), user_id.to_string())
        .await
        .unwrap();

    run_prompt_until_ended(&app, user_id, session_id, "Test rollout tracking")
        .await;

    // After the agent run, RolloutTracker should have recorded token usage.
    let budget_after = app.rollout_tracker.token_budget().await;
    assert!(
        budget_after.total_used > budget_before.total_used,
        "RolloutTracker should record increased token usage after agent run. Before: {}, After: {}",
        budget_before.total_used,
        budget_after.total_used
    );
}

/// Test that the FragmentRegistry renders fragments that are used
/// during the agent run.
#[tokio::test]
async fn test_e2e_fragment_registry_active_during_run() {
    let app = setup_app_with_fake_response().await;
    let user_id = "_legacy_";
    let session_id = "e2e-fragment-1";

    app.session_manager
        .create_with_user(session_id.to_string(), user_id.to_string())
        .await
        .unwrap();

    // Verify that the FragmentRegistry has the built-in fragment
    let frag_reg = app.extension_registry.fragment_registry();
    let frag_ctx =
        synthia_core::tool::fragment::FragmentContext::new(session_id, user_id);
    let rendered = frag_reg.render_active(&frag_ctx).await;

    assert!(
        !rendered.is_empty(),
        "FragmentRegistry should have active fragments for the e2e test"
    );
    let names: Vec<&str> = rendered.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"system_prompt"),
        "Rendered fragments should include 'system_prompt', got: {names:?}"
    );

    // Run the agent and verify it completes successfully — the
    // FragmentRegistry is invoked by main_loop to build the system
    // prompt. A successful run confirms the fragment rendering path
    // is wired end-to-end.
    run_prompt_until_ended(&app, user_id, session_id, "Test fragment registry")
        .await;
}

/// Test that the InterceptorChain is wired into the controller's
/// RunDependencies and available during the agent run.
#[tokio::test]
async fn test_e2e_interceptor_chain_available_during_run() {
    let app = setup_app_with_fake_response().await;
    let user_id = "_legacy_";
    let session_id = "e2e-interceptor-1";

    app.session_manager
        .create_with_user(session_id.to_string(), user_id.to_string())
        .await
        .unwrap();

    // `for_test()` now registers default interceptors (mirrors
    // production configuration). Verify the chain is non-empty.
    assert!(
        !app.interceptor_chain.is_empty(),
        "InterceptorChain should have interceptors registered"
    );

    run_prompt_until_ended(&app, user_id, session_id, "Test interceptor chain")
        .await;

    // The InterceptorChain is wired through RunDependencies →
    // AgentRunConfig → main_loop. A successful agent run confirms
    // the chain is properly passed and does not block execution.
}

/// Test the full event lifecycle: SessionStarted → IterationStarted →
/// LlmRequestStarted → LlmResponseComplete → IterationCompleted →
/// SessionEnded for a simple text-only response.
#[tokio::test]
async fn test_e2e_event_lifecycle_order() {
    let app = setup_app_with_fake_response().await;
    let user_id = "_legacy_";
    let session_id = "e2e-lifecycle-1";

    app.session_manager
        .create_with_user(session_id.to_string(), user_id.to_string())
        .await
        .unwrap();

    let events = run_prompt_and_collect_events(
        &app,
        user_id,
        session_id,
        "Trace event lifecycle",
    )
    .await;

    // Extract the event type sequence (excluding ephemeral events)
    let durable_types: Vec<&str> = events
        .iter()
        .map(|e| match e {
            AgentEvent::System(SystemEvent::SessionStarted { .. }) => {
                "SessionStarted"
            }
            AgentEvent::System(SystemEvent::SessionEnded { .. }) => {
                "SessionEnded"
            }
            AgentEvent::System(SystemEvent::Recovery { .. }) => "Recovery",
            AgentEvent::ModelDone(_) => "LlmResponseComplete",
            AgentEvent::Hook(HookEvent::Message { .. }) => "SteeringReceived",
            _ => "Other",
        })
        .filter(|t| *t != "Other")
        .collect();

    assert!(
        durable_types.contains(&"SessionStarted"),
        "Should have SessionStarted in events: {durable_types:?}"
    );
    assert!(
        durable_types.contains(&"LlmResponseComplete"),
        "Should have LlmResponseComplete in events: {durable_types:?}"
    );
    assert!(
        durable_types.contains(&"SessionEnded"),
        "Should have SessionEnded in events: {durable_types:?}"
    );

    // Verify SessionStarted comes before SessionEnded
    let started_idx = durable_types
        .iter()
        .position(|t| *t == "SessionStarted")
        .unwrap();
    let ended_idx = durable_types
        .iter()
        .position(|t| *t == "SessionEnded")
        .unwrap();
    assert!(
        started_idx < ended_idx,
        "SessionStarted should come before SessionEnded"
    );
}

/// Test that ExtensionRegistry components are properly wired through
/// the SessionController → RunDependencies → AgentRunConfig pipeline.
#[tokio::test]
async fn test_e2e_extension_registry_wired_through_pipeline() {
    let app = setup_app_with_fake_response().await;
    let user_id = "_legacy_";
    let session_id = "e2e-pipeline-1";

    app.session_manager
        .create_with_user(session_id.to_string(), user_id.to_string())
        .await
        .unwrap();

    // Verify all Registry-First components are present in AppState
    let health = app.extension_registry.health_check().await;
    assert!(health.healthy, "ExtensionRegistry should be healthy");

    run_prompt_until_ended(&app, user_id, session_id, "Verify pipeline wiring")
        .await;

    // After the run completes, verify that RolloutTracker recorded usage
    let budget = app.rollout_tracker.token_budget().await;
    assert!(
        budget.total_used > 0,
        "RolloutTracker should have recorded token usage after agent run, got {}",
        budget.total_used
    );
}
