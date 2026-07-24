use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use synthia_agent::{
    Agent,
    AgentConfig,
    AgentEvent,
    AgentInput,
    AgentRunConfig,
    SessionEndReason,
};
use synthia_context::{ProtectionZone, assembler::ContextAssembler};
use synthia_core::ApiResponse;
use synthia_provider::router::ModelRouter;
use synthia_session::Store as SessionStore;
use tokio_util::sync::CancellationToken;

use crate::{
    middleware::auth::RequestUserId,
    sse::sse_event_stream,
    state::AppState,
};

#[derive(Deserialize)]
pub struct ChatRequest {
    pub session_id: Option<String>,
    pub input: String,
    pub model: Option<String>,
    pub max_iterations: Option<usize>,
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub session_id: String,
    pub response: String,
    pub end_reason: String,
}

/// POST /api/chat
/// Accepts user input, initializes the agent, executes the ReAct loop,
/// collects the final assistant response, and returns it.
///
/// Supports content negotiation:
/// - `Accept: text/event-stream` -> SSE streaming response
/// - Other -> JSON response (existing behavior)
pub async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let wants_sse = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.contains("text/event-stream"))
        .unwrap_or(false);

    if wants_sse {
        return chat_sse_handler(State(state), Extension(user_id), Json(req))
            .await
            .into_response();
    }

    // Default: return JSON response (existing behavior)
    chat_json_handler(State(state), Extension(user_id), Json(req))
        .await
        .into_response()
}

/// Internal handler for SSE streaming chat.
async fn chat_sse_handler(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let session_id = req
        .session_id
        .clone()
        .unwrap_or_else(synthia_core::generate_session_id);

    state
        .session_manager
        .create_with_user(session_id.clone(), user_id.0.clone())
        .await
        .expect("auth layer always provides a non-empty user_id");

    // Create or get broadcaster for this session
    let broadcaster = state
        .get_or_create_broadcaster(&user_id.0, &session_id)
        .await;

    let provider = state.default_provider.clone();
    let tool_reg = {
        let guard = state.tool_registry.read().await;
        guard.clone()
    };

    let hooks = synthia_hook::HookRegistry::new();
    let model = req
        .model
        .clone()
        .unwrap_or_else(|| state.default_model.clone());
    let workspace_root = state.workspace_root.clone();
    // Clone broadcaster for the background task before moving it
    let broadcaster_for_task = broadcaster.clone();
    let session_id_clone = session_id.clone();
    let input = req.input.clone();
    let max_iterations = req.max_iterations.unwrap_or(20);

    // Spawn agent loop in background, sending events to broadcaster
    let user_id_string = user_id.0.clone();
    let user_id_for_cleanup = user_id_string.clone();
    tokio::spawn(async move {
        let config = AgentConfig {
            model,
            max_iterations,
            max_tokens: 4096,
            temperature: Some(0.7),
            workspace_root: workspace_root.clone(),
            token_budget: None,
            checkpoint_dir: None,
            context_token_budget: Some(
                synthia_session::types::TokenBudget::default(),
            ),
            ..Default::default()
        };

        // Create context assembler
        let protection_zone = ProtectionZone::default();
        let assembler = ContextAssembler::new(config.max_tokens)
            .with_protection_zone(protection_zone);

        // Create session store
        let session_store_dir =
            workspace_root.join(".synthia").join("sessions");
        let session_store = SessionStore::new(session_store_dir);

        // Create model router as Arc
        let model_router = Arc::new(ModelRouter::new());

        let cancel_token = CancellationToken::new();

        let agent_stream = Agent::run_stream(AgentRunConfig {
            provider,
            tool_registry: tool_reg,
            hook_registry: Arc::new(hooks),
            model_router,
            // user_id is resolved by the auth middleware from the
            // request's API key (or `SERVER_DEFAULT_USER_ID` if no
            // key is configured) and surfaced via RequestUserId.
            user_id: user_id_string,
            session_id: session_id_clone.clone(),
            input: AgentInput::text(&input),
            config,
            context_assembler: Some(Arc::new(assembler)),
            session_store,
            steering_channel: None,
            cancel_token,
            memory_event_sender: None,
            agent_control: None,
            fork_policy: Default::default(),
            compaction_provider: None,
            session_input_queue: None,
            subagent_session_factory: None,
            approval_service: Some(state.approval_service.clone()),
            sandbox_manager: Some(state.sandbox_manager.clone()),
            tool_orchestrator: Some(state.tool_orchestrator.clone()),
            guardian_coordinator: None,
        });

        let mut event_stream = agent_stream;
        while let Some(event) = event_stream.next().await {
            let is_terminal = matches!(event, AgentEvent::SessionEnded { .. });
            if let Err(e) = broadcaster_for_task.send(event) {
                tracing::warn!(error = %e, "Failed to send event to SSE subscribers");
                break;
            }
            if is_terminal {
                break;
            }
        }

        // Clean up broadcaster when done
        state
            .remove_broadcaster(&user_id_for_cleanup, &session_id_clone)
            .await;
    });

    // Return SSE stream from subscriber
    let rx = broadcaster.subscribe();
    let mut response = sse_event_stream(rx).into_response();
    response
        .headers_mut()
        .insert("Deprecation", "true".parse().unwrap());
    response
}

/// Internal handler for JSON response chat (existing behavior).
async fn chat_json_handler(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let session_id = req
        .session_id
        .unwrap_or_else(synthia_core::generate_session_id);

    state
        .session_manager
        .create_with_user(session_id.clone(), user_id.0.clone())
        .await
        .expect("auth layer always provides a non-empty user_id");

    let provider = state.default_provider.clone();
    let tool_reg = {
        let guard = state.tool_registry.read().await;
        guard.clone()
    };

    // Create a fresh HookRegistry with the built-in security hooks
    let hooks = synthia_hook::HookRegistry::new();

    let model = req
        .model
        .clone()
        .unwrap_or_else(|| state.default_model.clone());
    let workspace_root = state.workspace_root.clone();

    let config = AgentConfig {
        model,
        max_iterations: req.max_iterations.unwrap_or(20),
        max_tokens: 4096,
        temperature: Some(0.7),
        workspace_root: workspace_root.clone(),
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(
            synthia_session::types::TokenBudget::default(),
        ),
        ..Default::default()
    };

    // Create context assembler
    let protection_zone = ProtectionZone::default();
    let assembler = ContextAssembler::new(config.max_tokens)
        .with_protection_zone(protection_zone);

    // Create session store
    let session_store_dir = workspace_root.join(".synthia").join("sessions");
    let session_store = SessionStore::new(session_store_dir);

    // Create model router as Arc
    let model_router = Arc::new(ModelRouter::new());

    let cancel_token = CancellationToken::new();

    let agent_stream = Agent::run_stream(AgentRunConfig {
        provider,
        tool_registry: tool_reg,
        hook_registry: Arc::new(hooks),
        model_router,
        // user_id is resolved by the auth middleware from the
        // request's API key (or `SERVER_DEFAULT_USER_ID` if no
        // key is configured) and surfaced via RequestUserId.
        user_id: user_id.0,
        session_id: session_id.clone(),
        input: AgentInput::text(&req.input),
        config,
        context_assembler: Some(Arc::new(assembler)),
        session_store,
        steering_channel: None,
        cancel_token,
        memory_event_sender: None,
        agent_control: None,
        fork_policy: Default::default(),
        compaction_provider: None,
        session_input_queue: None,
        subagent_session_factory: None,
        approval_service: Some(state.approval_service.clone()),
        sandbox_manager: Some(state.sandbox_manager.clone()),
        tool_orchestrator: Some(state.tool_orchestrator.clone()),
        guardian_coordinator: None,
    });

    let mut assistant_response = String::new();
    let mut end_reason = "completed".to_string();

    let mut event_stream = agent_stream;
    while let Some(event) = event_stream.next().await {
        match event {
            AgentEvent::LlmStreamDelta { content } => {
                assistant_response.push_str(&content);
            }
            AgentEvent::LlmResponseComplete { content, .. }
                if assistant_response.is_empty() =>
            {
                assistant_response = content;
            }
            AgentEvent::SessionEnded { reason } => {
                end_reason = match reason {
                    SessionEndReason::Completed => "completed".to_string(),
                    SessionEndReason::Cancelled => "cancelled".to_string(),
                    SessionEndReason::Error(e) => format!("error: {}", e),
                    SessionEndReason::TokenBudgetExceeded => {
                        "token_budget_exceeded".to_string()
                    }
                    SessionEndReason::MaxIterationsReached => {
                        "max_iterations_reached".to_string()
                    }
                    SessionEndReason::GuardianBlocked => {
                        "guardian_blocked".to_string()
                    }
                    SessionEndReason::LoopDetected => {
                        "loop_detected".to_string()
                    }
                    SessionEndReason::CircuitBreakerOpen => {
                        "circuit_breaker_open".to_string()
                    }
                };
                break;
            }
            _ => {}
        }
    }

    tracing::info!(
        session_id = %session_id,
        end_reason = %end_reason,
        response_len = assistant_response.len(),
        "Chat request completed"
    );

    let mut headers = HeaderMap::new();
    headers.insert("Deprecation", "true".parse().unwrap());
    (
        headers,
        Json(ApiResponse::ok(ChatResponse {
            session_id,
            response: assistant_response,
            end_reason,
        })),
    )
}
