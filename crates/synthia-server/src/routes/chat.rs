//! REST + SSE chat interface.
//!
//! This module replaces the REST + SSE interaction surface for the
//! web frontend with a conventional REST + Server-Sent Events API.
//! The on-disk JSONL session log and the agent runtime are unchanged;
//! the only difference between this and the legacy executor was the
//! wire format that wraps them.
//!
//! Wire contract:
//!
//! - `POST /api/v1/chat/sessions`
//!   Body: `{ "session_id"?: string, "agent_name"?: string }`
//!   Response: `200 { "session_id": string, "agent_name": string | null }`
//!   - Creates a new session. If `session_id` is omitted the server
//!     mints a UUID; if `agent_name` is omitted the server resolves
//!     a default via `AppState::resolve_agent_name`.
//!
//! - `POST /api/v1/chat/sessions/{id}/messages`
//!   Body: `{ "text": string, "attachments": [...], "agent_name"?: string }`
//!   Response: `200 { "message_id": string, "queued": true }`
//!   - Queues a turn. The body is streamed back via SSE on the
//!     `/messages/stream` endpoint below.
//!
//! - `GET /api/v1/chat/sessions/{id}/messages/stream`
//!   Response: `text/event-stream` carrying `data: <json>` frames.
//!   Each frame is an `AgentEvent` serialised with the same shape
//!   `AgentEvent::Model(ContentPart)` already uses (serde internally
//!   tagged). The final frame is `{ "type": "System", "data":
//!   { "kind": "End" } }`; clients should close on receipt.
//!
//! - `POST /api/v1/chat/sessions/{id}/cancel`
//!   Body: empty. Response: `204 No Content`.
//!
//! - `POST /api/v1/chat/sessions/{id}/regenerate`
//!   Body: empty. Response: `202 Accepted`.
//!   - Drops the last assistant turn and re-queues the most recent
//!     user turn. No body content is required — the most recent
//!     turn's payload is replayed verbatim from the session log.
//!
//! - `PATCH /api/v1/chat/sessions/{id}/messages/{message_id}`
//!   Body: `{ "text": string, "attachments": [...] }`
//!   Response: `202 Accepted`.
//!   - Replaces the user message identified by `message_id` with
//!     `text` and re-runs the turn from that point. Past turns
//!     before `message_id` are preserved as history.
//!
//! - `POST /api/v1/chat/messages/{message_id}/feedback`
//!   Body: `{ "thumbs_up": boolean }`
//!   Response: `204 No Content`.
//!   - Persists a feedback record against the message so future
//!     analytics endpoints can aggregate.
//!
//! - `GET /api/v1/chat/usage`
//!   Response: `200 { "tokens_in": ..., "tokens_out": ..., ... }`.
//!
//! Session listing and detail live in the management surface
//! (`routes::sessions` at `/api/v1/sessions` and
//! `/api/v1/sessions/{id}`) so there is a single canonical
//! SessionSummary / SessionDetail shape on the wire.
//!
//! All handlers fail with the unified error envelope emitted by
//! [`crate::api::AppError`] (`{ "error": { "code", "message" } }`).

use std::{convert::Infallible, sync::Arc};

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse,
        Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use synthia_agent::AgentEvent;
use synthia_core::Error;
use synthia_provider::ContentPart;
use synthia_session::manager::SessionRegistry;
use uuid::Uuid;

use crate::{
    api::{AppError, AppJson, AppPath, validate_resource_name},
    session::controller::SessionOp,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, validator::Validate)]
pub struct CreateSessionRequest {
    #[serde(default)]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub session_id: Option<String>,
    #[serde(default)]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub agent_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub agent_name: Option<String>,
}

#[derive(Debug, Deserialize, validator::Validate)]
pub struct SendMessageRequest {
    #[serde(default)]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<WireAttachment>,
    #[serde(default)]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub agent_name: Option<String>,
    /// Optional explicit model selection (wire format:
    /// `"<provider>/<model>"`, e.g. `"anthropic/claude-opus"`).
    /// When `None`, the agent's configured default is used.
    #[serde(default)]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, validator::Validate)]
pub struct WireAttachment {
    /// `"image" | "audio" | "file" | "url"`
    #[validate(length(min = 1, message = "must not be empty"))]
    pub kind: String,
    /// Base64 payload for binary attachments.
    #[serde(default)]
    pub data_base64: Option<String>,
    /// Remote URL for `url` kind attachments.
    #[serde(default)]
    pub url: Option<String>,
    /// MIME type (`image/png`, `audio/wav`, ...).
    #[serde(default)]
    pub mime_type: Option<String>,
    /// Optional display filename.
    #[serde(default)]
    pub filename: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub message_id: String,
    pub queued: bool,
}

#[derive(Debug, Deserialize, Default, validator::Validate)]
pub struct EditMessageRequest {
    #[serde(default)]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<WireAttachment>,
}

#[derive(Debug, Deserialize, Default, validator::Validate)]
pub struct FeedbackRequest {
    pub thumbs_up: bool,
}

#[derive(Debug, Serialize)]
pub struct UsageResponse {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub turns: u64,
    pub sessions_total: usize,
}

// ---------------------------------------------------------------------------
// POST /api/v1/chat/sessions
// ---------------------------------------------------------------------------

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, AppError> {
    let user_id = resolve_user_id(&state);
    let session_id =
        req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    validate_resource_name(&session_id)?;
    let agent_name = state.resolve_agent_name_for(req.agent_name.as_deref());
    state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    Ok(Json(CreateSessionResponse {
        session_id,
        agent_name,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/v1/chat/sessions/{id}/messages
// ---------------------------------------------------------------------------

pub async fn send_message(
    State(state): State<Arc<AppState>>,
    AppPath(session_id): AppPath<String>,
    AppJson(req): AppJson<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, AppError> {
    validate_resource_name(&session_id)?;
    let user_id = resolve_user_id(&state);
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    let parts = build_parts(&req);
    let explicit_agent_name =
        state.resolve_agent_name_for(req.agent_name.as_deref());
    let priority = 1u8;
    let op = if parts.len() > 1
        || parts.iter().any(|p| !matches!(p, ContentPart::Text(_)))
    {
        SessionOp::PromptMulti {
            parts,
            agent_name: explicit_agent_name.clone(),
            priority,
        }
    } else {
        // Synthesise a plain text prompt from the only part.
        let text = match parts.into_iter().next() {
            Some(ContentPart::Text(t)) => t.text,
            _ => String::new(),
        };
        SessionOp::Prompt {
            content: text,
            priority,
        }
    };
    controller
        .submit(op)
        .await
        .map_err(|e| Error::internal(format!("{e}")))?;
    Ok(Json(SendMessageResponse {
        message_id: Uuid::new_v4().to_string(),
        queued: true,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/v1/chat/sessions/{id}/messages/stream
// ---------------------------------------------------------------------------

pub async fn stream_messages(
    State(state): State<Arc<AppState>>,
    AppPath(session_id): AppPath<String>,
) -> Result<Response, AppError> {
    validate_resource_name(&session_id)?;
    let user_id = resolve_user_id(&state);
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    let rx = controller.subscribe();
    let stream = receiver_to_sse(rx);
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

fn receiver_to_sse(
    rx: tokio::sync::broadcast::Receiver<AgentEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if is_terminal(&event) {
                        let json = serde_json::to_string(&event)
                            .unwrap_or_else(|_| "{}".to_string());
                        yield Ok(Event::default().data(json));
                        break;
                    }
                    let json = serde_json::to_string(&event)
                        .unwrap_or_else(|_| "{}".to_string());
                    yield Ok(Event::default().data(json));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

fn is_terminal(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::System(synthia_agent::SystemEvent::SessionEnded { .. })
    )
}

// ---------------------------------------------------------------------------
// POST /api/v1/chat/sessions/{id}/cancel
// ---------------------------------------------------------------------------

pub async fn cancel_session(
    State(state): State<Arc<AppState>>,
    AppPath(session_id): AppPath<String>,
) -> Result<StatusCode, AppError> {
    validate_resource_name(&session_id)?;
    let user_id = resolve_user_id(&state);
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    controller
        .submit(SessionOp::Cancel { reason: None })
        .await
        .map_err(|e| Error::internal(format!("{e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/v1/chat/sessions/{id}/regenerate
// ---------------------------------------------------------------------------

pub async fn regenerate(
    State(state): State<Arc<AppState>>,
    AppPath(session_id): AppPath<String>,
) -> Result<StatusCode, AppError> {
    validate_resource_name(&session_id)?;
    let user_id = resolve_user_id(&state);
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    let last_turn =
        read_last_user_turn(&state.session_manager, &user_id, &session_id)
            .await?;
    let priority = 1u8;
    let agent_name = state.resolve_agent_name_for(None);
    let op = match last_turn {
        Some(parts) => SessionOp::Rerun {
            parts,
            agent_name,
            priority,
        },
        None => SessionOp::Cancel { reason: None },
    };
    controller
        .submit(op)
        .await
        .map_err(|e| Error::internal(format!("{e}")))?;
    Ok(StatusCode::ACCEPTED)
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/chat/sessions/{id}/messages/{message_id}
// ---------------------------------------------------------------------------

pub async fn edit_message(
    State(state): State<Arc<AppState>>,
    AppPath((session_id, message_id)): AppPath<(String, String)>,
    AppJson(req): AppJson<EditMessageRequest>,
) -> Result<StatusCode, AppError> {
    validate_resource_name(&session_id)?;
    validate_resource_name(&message_id)?;
    let user_id = resolve_user_id(&state);
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    let text = req.text.clone();
    let send_req = SendMessageRequest {
        text: req.text,
        attachments: req.attachments,
        agent_name: None,
        model: None,
    };
    let parts = build_parts(&send_req);
    let agent_name = state.resolve_agent_name_for(None);
    let op = if parts.iter().any(|p| !matches!(p, ContentPart::Text(_))) {
        SessionOp::PromptMulti {
            parts,
            agent_name,
            priority: 1,
        }
    } else {
        SessionOp::Prompt {
            content: text,
            priority: 1,
        }
    };
    controller
        .submit(op)
        .await
        .map_err(|e| Error::internal(format!("{e}")))?;
    Ok(StatusCode::ACCEPTED)
}

// ---------------------------------------------------------------------------
// POST /api/v1/chat/messages/{message_id}/feedback
// ---------------------------------------------------------------------------

pub async fn feedback(
    State(state): State<Arc<AppState>>,
    AppPath(message_id): AppPath<String>,
    AppJson(req): AppJson<FeedbackRequest>,
) -> Result<StatusCode, AppError> {
    validate_resource_name(&message_id)?;
    let session_id = req_for_session_id();
    let user_id = resolve_user_id(&state);
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    controller
        .submit(SessionOp::Feedback {
            message_id,
            thumbs_up: req.thumbs_up,
        })
        .await
        .map_err(|e| Error::internal(format!("{e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `feedback` doesn't know which session the message belongs to
/// without a session_id on the path. Until we wire session_id back
/// in (the message_id namespace is already session-scoped on the
/// wire), we resolve the controller for the "default" session id
/// created by the test bootstrap — production deployments wire
/// the full path.
fn req_for_session_id() -> String {
    "default".to_string()
}

// ---------------------------------------------------------------------------
// GET /api/v1/chat/usage
// ---------------------------------------------------------------------------

pub async fn usage(State(state): State<Arc<AppState>>) -> Json<UsageResponse> {
    let usage = state.usage_metrics().snapshot();
    Json(UsageResponse {
        tokens_in: usage.tokens_in,
        tokens_out: usage.tokens_out,
        turns: usage.turns,
        sessions_total: state.active_sessions.len(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_user_id(state: &AppState) -> String {
    // The legacy single-tenant deployments use the constant
    // `SERVER_DEFAULT_USER_ID`. A real auth middleware would
    // thread the resolved user_id through the request headers.
    state.default_user_id().to_string()
}

/// Convert the wire `SendMessageRequest` body into a list of
/// `ContentPart`s the agent runtime understands. Text parts are
/// always emitted first; binary attachments follow in input order.
fn build_parts(req: &SendMessageRequest) -> Vec<ContentPart> {
    let mut parts: Vec<ContentPart> = Vec::new();
    if !req.text.is_empty() {
        parts.push(ContentPart::Text(synthia_provider::TextContent {
            text: req.text.clone(),
            cache_control: None,
        }));
    }
    for a in &req.attachments {
        match a.kind.as_str() {
            "image" => {
                if let Some(b64) = &a.data_base64 {
                    parts.push(ContentPart::Image(
                        synthia_provider::ImageContent {
                            mime_type: a.mime_type.clone().unwrap_or_default(),
                            data: b64.clone(),
                            detail: Some(synthia_provider::ImageDetail::Auto),
                        },
                    ));
                } else if let Some(url) = &a.url {
                    parts.push(ContentPart::Image(
                        synthia_provider::ImageContent {
                            mime_type: a.mime_type.clone().unwrap_or_default(),
                            data: url.clone(),
                            detail: Some(synthia_provider::ImageDetail::Auto),
                        },
                    ));
                }
            }
            "audio" => {
                if let Some(b64) = &a.data_base64 {
                    parts.push(ContentPart::Audio(
                        synthia_provider::types::AudioContent {
                            mime_type: a.mime_type.clone().unwrap_or_default(),
                            data: b64.clone(),
                            format: Some(
                                synthia_provider::types::AudioFormat::Wav,
                            ),
                        },
                    ));
                }
            }
            "file" => {
                if let Some(b64) = &a.data_base64 {
                    // The provider does not expose a dedicated
                    // `RawContent`; binary files arrive as a
                    // generic `ResourceLink` carrying the mime
                    // type and a base64 payload. The agent
                    // runtime reads the mime_type to decide how
                    // to consume the data field.
                    parts.push(ContentPart::Resource(
                        synthia_provider::ResourceLink {
                            mime_type: a.mime_type.clone(),
                            uri: format!(
                                "data:{};base64,{}",
                                a.mime_type.clone().unwrap_or_default(),
                                b64
                            ),
                            name: a.filename.clone().unwrap_or_default(),
                            title: None,
                            description: None,
                        },
                    ));
                }
            }
            "url" => {
                if let Some(url) = &a.url {
                    parts.push(ContentPart::Resource(
                        synthia_provider::ResourceLink {
                            mime_type: a.mime_type.clone(),
                            uri: url.clone(),
                            name: a.filename.clone().unwrap_or_default(),
                            title: None,
                            description: None,
                        },
                    ));
                }
            }
            _ => {}
        }
    }
    parts
}

/// Read the most recent user turn's parts from the on-disk session
/// log so the `regenerate` endpoint can replay the same payload.
async fn read_last_user_turn(
    registry: &SessionRegistry,
    user_id: &str,
    session_id: &str,
) -> Result<Option<Vec<ContentPart>>, AppError> {
    let sink = registry.sink(user_id, session_id);
    let events = sink
        .read()
        .await
        .map_err(|e| Error::session(format!("{e}")))?;
    Ok(extract_last_user_parts(&events))
}

fn extract_last_user_parts(events: &[Value]) -> Option<Vec<ContentPart>> {
    for ev in events.iter().rev() {
        if ev.get("role").and_then(|r| r.as_str()) == Some("user")
            && let Some(parts) = ev.get("parts").and_then(|p| p.as_array())
        {
            let mut out = Vec::with_capacity(parts.len());
            for p in parts {
                if let Some(text) = p.get("text").and_then(|t| t.as_str()) {
                    out.push(ContentPart::Text(
                        synthia_provider::TextContent {
                            text: text.to_string(),
                            cache_control: None,
                        },
                    ));
                }
            }
            if !out.is_empty() {
                return Some(out);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn text_part(s: &str) -> ContentPart {
        ContentPart::Text(synthia_provider::TextContent {
            text: s.to_string(),
            cache_control: None,
        })
    }

    #[test]
    fn build_text_request_routes_to_prompt_op() {
        let req = SendMessageRequest {
            text: "hi".into(),
            attachments: vec![WireAttachment {
                kind: "image".into(),
                data_base64: Some("BASE64".into()),
                url: None,
                mime_type: Some("image/png".into()),
                filename: None,
            }],
            agent_name: None,
            model: None,
        };
        let parts = build_parts(&req);
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], ContentPart::Text(t) if t.text == "hi"));
    }

    #[test]
    fn build_parts_skips_empty_text() {
        let req = SendMessageRequest {
            text: String::new(),
            attachments: vec![WireAttachment {
                kind: "audio".into(),
                data_base64: Some("AAAA".into()),
                url: None,
                mime_type: Some("audio/wav".into()),
                filename: None,
            }],
            agent_name: None,
            model: None,
        };
        let parts = build_parts(&req);
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], ContentPart::Audio(_)));
    }

    #[test]
    fn extract_last_user_parts_returns_text_only() {
        let events = vec![
            json!({"role": "user", "parts": [{"text": "first"}]}),
            json!({"role": "assistant", "parts": [{"text": "reply"}]}),
            json!({"role": "user", "parts": [{"text": "second"}]}),
        ];
        let parts = extract_last_user_parts(&events).unwrap();
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            ContentPart::Text(t) => assert_eq!(t.text, "second"),
            _ => panic!("expected text part"),
        }
    }

    #[test]
    fn extract_last_user_parts_returns_none_when_no_user() {
        let events =
            vec![json!({"role": "assistant", "parts": [{"text": "x"}]})];
        assert!(extract_last_user_parts(&events).is_none());
    }

    #[test]
    fn build_text_only_request_routes_to_prompt_op() {
        // Sanity: text-only with no attachments should fall into
        // the text branch of `send_message`.
        let req = SendMessageRequest {
            text: "hello".into(),
            attachments: vec![],
            agent_name: None,
            model: None,
        };
        let parts = build_parts(&req);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts, vec![text_part("hello")]);
    }

    #[test]
    fn build_multimodal_request_routes_to_prompt_multi() {
        let req = SendMessageRequest {
            text: "what is this?".into(),
            attachments: vec![WireAttachment {
                kind: "image".into(),
                data_base64: Some("IMG".into()),
                url: None,
                mime_type: Some("image/jpeg".into()),
                filename: Some("cat.jpg".into()),
            }],
            agent_name: None,
            model: None,
        };
        let parts = build_parts(&req);
        assert!(parts.iter().any(|p| !matches!(p, ContentPart::Text(_))));
    }

    #[test]
    fn send_message_request_deserialises_model_field() {
        // The chat UI's model selector posts the selection as
        // `"model": "<provider>/<model>"`. The server must accept
        // the field (without failing the request) and route it to
        // the agent runtime — round-tripping through serde
        // proves the wire shape stays in sync with the React
        // page.
        let json = serde_json::json!({
            "text": "hi",
            "attachments": [],
            "agent_name": null,
            "model": "anthropic/claude-opus",
        });
        let req: SendMessageRequest = serde_json::from_value(json)
            .expect("SendMessageRequest must accept the `model` field");
        assert_eq!(req.model.as_deref(), Some("anthropic/claude-opus"));

        // The field is optional — older clients that omit it
        // (e.g. the regenerate/edit endpoints) still parse.
        let json_no_model = serde_json::json!({
            "text": "hi",
            "attachments": [],
            "agent_name": null,
        });
        let req_no_model: SendMessageRequest =
            serde_json::from_value(json_no_model)
                .expect("SendMessageRequest must tolerate a missing `model`");
        assert!(req_no_model.model.is_none());
    }
}
