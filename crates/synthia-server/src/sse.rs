//! Server-Sent Events (SSE) streaming support for agent events.
//!
//! Converts a `broadcast::Receiver<AgentEvent>` into an axum `Sse` response,
//! with heartbeat, error handling, and connection cleanup.

use std::{convert::Infallible, pin::Pin, time::Duration};

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures::{Stream, StreamExt as _};
use serde::Serialize;
use synthia_agent::events::{
    AgentEvent,
    AgentOutput,
    HookEvent,
    SystemEvent,
    WarningKind,
};
use synthia_provider::ContentPart;
use tokio_stream::wrappers::BroadcastStream;

/// SSE heartbeat interval.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// Type alias for the SSE stream type using trait objects to avoid
/// unstable `impl Trait` in type alias position.
pub type SseStream =
    Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>>;

/// An error that can occur during SSE streaming.
#[derive(Debug, Serialize)]
pub struct SseError {
    pub code: String,
    pub message: String,
}

/// Converts an `AgentEvent` to an SSE event string.
///
/// The event variant name is used as the SSE event name,
/// and the JSON-serialized event is used as the data field.
pub fn agent_event_to_sse(event: &AgentEvent) -> SseEvent {
    let event_type = event_variant_name(event);
    let json = serde_json::to_string(event).unwrap_or_default();
    SseEvent::default().event(event_type).data(&json)
}

/// Extracts the variant name from an `AgentEvent` for use as the SSE event name.
pub fn event_variant_name(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::Model(part) => match part {
            ContentPart::Text(_) => "ModelText",
            ContentPart::Reasoning(_) => "ModelReasoning",
            ContentPart::ToolUse(_) => "ToolCallStarted",
            ContentPart::ToolResult(_) => "ToolCallCompleted",
            ContentPart::Image(_) => "ModelImage",
            ContentPart::Audio(_) => "ModelAudio",
            ContentPart::Resource(_) => "ModelResource",
        },
        AgentEvent::ModelDone(_) => "ModelDone",
        AgentEvent::System(sys) => match sys {
            SystemEvent::SessionStarted { .. } => "SessionStarted",
            SystemEvent::SessionEnded { .. } => "SessionEnded",
            SystemEvent::SessionInterrupted { .. } => "SessionInterrupted",
            SystemEvent::Progress { .. } => "Progress",
            SystemEvent::Warning { kind, .. } => match kind {
                WarningKind::Guardian => "GuardianWarning",
                WarningKind::Loop => "LoopWarning",
                WarningKind::TokenBudget => "TokenBudgetWarning",
                WarningKind::ContextCompaction => "ContextCompacted",
                WarningKind::Hook => "HookError",
                WarningKind::EditConflict => "EditConflict",
            },
            SystemEvent::Recovery { .. } => "RecoveryApplied",
            SystemEvent::Usage { .. } => "TokenBudgetNotice",
        },
        AgentEvent::Agent(_, inner) => event_variant_name(inner),
        AgentEvent::Hook(hook) => match hook {
            HookEvent::Message { .. } => "SteeringReceived",
            HookEvent::ConfirmRequest { .. } => "GuardianConfirmationRequest",
            HookEvent::ConfirmResponse { .. } => "GuardianConfirmationResponse",
            HookEvent::Custom { .. } => "Custom",
        },
    }
}

/// Creates an SSE error event.
pub fn error_event(code: &str, message: &str) -> SseEvent {
    let error = SseError {
        code: code.to_string(),
        message: message.to_string(),
    };
    let json = serde_json::to_string(&error).unwrap_or_default();
    SseEvent::default().event("error").data(&json)
}

/// Creates an SSE response from a broadcast receiver of agent events.
///
/// The returned stream interleaves agent events with heartbeat comments
/// sent every 15 seconds. When the agent stream ends (e.g., SessionEnded),
/// the SSE connection is gracefully closed.
pub fn sse_event_stream(
    rx: tokio::sync::broadcast::Receiver<AgentEvent>,
) -> impl axum::response::IntoResponse {
    let agent_stream: SseStream = BroadcastStream::new(rx)
        .filter_map(|result: Result<AgentEvent, _>| async move {
            match result {
                Ok(event) => Some(Ok(agent_event_to_sse(&event))),
                Err(_) => None,
            }
        })
        .boxed();

    let heartbeat: SseStream = async_stream::stream! {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            interval.tick().await;
            yield Ok(SseEvent::default().comment("ping"));
        }
    }
    .boxed();

    let merged =
        tokio_stream::StreamExt::merge(agent_stream, heartbeat).boxed();

    Sse::new(merged).keep_alive(
        KeepAlive::new().interval(HEARTBEAT_INTERVAL).text(": ping"),
    )
}

/// Creates an SSE response from an `AgentOutput` stream.
///
/// Converts each `AgentEvent` from the stream into a properly formatted SSE event
/// with `Content-Type: text/event-stream` and `data:` prefixes.
pub fn agent_output_to_sse(
    output: AgentOutput,
) -> impl axum::response::IntoResponse {
    let agent_stream: SseStream = output
        .map(|event: AgentEvent| Ok(agent_event_to_sse(&event)))
        .boxed();

    Sse::new(agent_stream).keep_alive(
        KeepAlive::new().interval(HEARTBEAT_INTERVAL).text(": ping"),
    )
}
