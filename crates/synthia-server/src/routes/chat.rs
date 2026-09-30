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
//!   - Re-queues the most recent user turn, recovered from the
//!     session log. Only the turn's **text** is recoverable: image
//!     and audio bytes are deliberately not serialised into the sink
//!     (see `synthia-server/src/session/controller.rs`), so a
//!     regenerated multimodal turn replays its prompt text without
//!     the attachment.
//!
//! - `POST /api/v1/chat/messages/{message_id}/feedback`
//!   Body: `{ "thumbs_up": boolean, "session_id": string }`
//!   Response: `204 No Content`.
//!   - Persists a feedback record into the *named session's* log so
//!     future analytics endpoints can aggregate it. `session_id` is
//!     required: the message id is a client-side label and cannot
//!     identify a session on its own.
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
use base64::Engine as _;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use synthia::{
    core::Error,
    harness::AgentEvent,
    provider::{ContentPart, Message, Role, traits::ModelProvider},
    session::manager::SessionRegistry,
};
use uuid::Uuid;

use crate::{
    api::{
        AppError,
        AppJson,
        AppPath,
        AppQuery,
        OperationRequest,
        validate_resource_name,
    },
    session::controller::{PinnedProvider, SessionOp},
    state::{AppState, ModelSelectionError},
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

#[derive(Debug, Default, Deserialize, validator::Validate)]
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

#[derive(Debug, Default, Deserialize, Serialize, validator::Validate)]
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
    pub filename: Option<String>,
    /// Attachment hash for `kind = "attachment_ref"` — the
    /// client references a previously-uploaded attachment by
    /// hash; the chat pipeline resolves it through
    /// `AppState::attachment_store` at the agent-runtime
    /// boundary (replay-safe via content-addressing).
    #[serde(default)]
    pub attachment_hash: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub message_id: String,
    pub queued: bool,
}

#[derive(Debug, Deserialize, Default, validator::Validate)]
pub struct FeedbackRequest {
    pub thumbs_up: bool,
    /// The session the rated message belongs to.
    ///
    /// `message_id` alone cannot resolve it — the id the chat UI
    /// sends is a client-side label, and the sink has no
    /// message-id index. Without this the handler resolved a
    /// hardcoded `"default"` session, so every 👍/👎 landed in the
    /// wrong log (and created that session on first click).
    #[serde(default)]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub session_id: String,
}
#[derive(Debug, Serialize)]
pub struct UsageResponse {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub turns: u64,
    pub sessions_total: usize,
}

/// Optional resume cursor for the SSE stream. `from = None` and
#[derive(Debug, Default, Deserialize, validator::Validate)]
pub struct StreamQuery {
    #[serde(default)]
    pub from: Option<String>,
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
    // Resolve the model selector BEFORE anything is queued: a
    // selection the server cannot honour is the caller's to see, and
    // discovering it only after the turn was accepted would leave the
    // user watching a stream that already ran the wrong model.
    let provider = state
        .resolve_provider(req.model.as_deref())
        .map_err(selection_error)?;
    let parts = build_parts(&req, Some(&state.attachment_store));
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
    submit_session_op(&state, &session_id, op, Some(provider)).await
}

/// Map a failed `model` resolution onto the unified error envelope.
///
/// The status comes from [`ModelSelectionError::status`] — `400` for a
/// value no configured provider resolves, `503` for a provider the
/// deployment configures but could not build — so the chat surface and
/// the Anthropic wire answer the same fault the same way. The message
/// is the error's own `Display`, which names the value the caller
/// sent.
fn selection_error(error: ModelSelectionError) -> AppError {
    let message = error.to_string();
    AppError::new(error.status(), Error::validation(message.clone()))
        .with_code("invalid_model")
        .with_message(message)
}

/// Resolve the session's controller and submit `op`, returning
/// the standard queued-turn envelope.
///
/// Shared by [`send_message`] and [`operation`] so both routes
/// go through one validation + controller-resolution path.
/// `provider` is the submitting route's `model` selection, already
/// resolved; `None` runs the session's configured default.
async fn submit_session_op(
    state: &Arc<AppState>,
    session_id: &str,
    op: SessionOp,
    provider: Option<Arc<dyn ModelProvider>>,
) -> Result<Json<SendMessageResponse>, AppError> {
    validate_resource_name(session_id)?;
    let user_id = resolve_user_id(state);
    let controller = state
        .get_or_create_session_controller(&user_id, session_id)
        .await?;
    controller
        .submit_with_provider(op, provider.map(PinnedProvider::new))
        .await
        .map_err(|e| Error::internal(format!("{e}")))?;
    Ok(Json(SendMessageResponse {
        message_id: Uuid::new_v4().to_string(),
        queued: true,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/v1/chat/sessions/{id}/operation  (R29, gated)
// ---------------------------------------------------------------------------

/// `POST /api/v1/chat/sessions/{id}/operation` — the
/// discriminated-union operation endpoint.
///
/// Registered only when `[operations] enabled = true` in
/// `config.toml` (see [`crate::config::OperationEndpointConfig`]).
///
/// [`OperationRequest::Prompt`] is dispatched to the same code
/// path [`send_message`] uses. The other variants are declared for
/// wire compatibility and answer `501 Not Implemented` with the
/// standard error envelope until the backing features land.
pub async fn operation(
    State(state): State<Arc<AppState>>,
    AppPath(session_id): AppPath<String>,
    AppJson(req): AppJson<OperationRequest>,
) -> Result<Json<SendMessageResponse>, AppError> {
    match req {
        OperationRequest::Prompt { text } => {
            let op = SessionOp::Prompt {
                content: text,
                priority: 1,
            };
            // The operation wire carries no model selection, so the
            // turn runs the session's configured default.
            submit_session_op(&state, &session_id, op, None).await
        }
        other => {
            let kind = other.kind();
            Err(AppError::new(
                StatusCode::NOT_IMPLEMENTED,
                Error::internal(format!(
                    "operation kind `{kind}` not implemented"
                )),
            )
            .with_code("not_implemented")
            .with_message(format!(
                "operation kind `{kind}` is not implemented"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
pub async fn stream_messages(
    State(state): State<Arc<AppState>>,
    AppPath(session_id): AppPath<String>,
    AppQuery(query): AppQuery<StreamQuery>,
) -> Result<Response, AppError> {
    validate_resource_name(&session_id)?;
    let user_id = resolve_user_id(&state);
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    // `from` is `None` or empty → today's behaviour
    // (byte-identical to the pre-resume wire shape). Any
    // other value must parse as `u64`; the handler rejects
    // an unparseable cursor with `400 Bad Request` so the
    // client never silently subscribes to a wrong window.
    let from = parse_resume_cursor(query.from.as_deref())?;
    // `from = None` and `from = 0` both take the resume
    // path: a fresh client (no cursor known) needs the
    // snapshot + cursor frames to learn the current
    // position before the live tail starts. The legacy
    // `receiver_to_sse` path was removed when the
    // stream-index protocol landed
    // (`2026-09-24-stream-index.md` §4.4): a client that
    // did not see a cursor could not resume a working
    // session because the only way to learn the cursor
    // was to send a new turn.
    let from = from.unwrap_or(0);
    let rx = controller.subscribe();
    let sink = state.session_manager.sink(&user_id, &session_id);
    let snapshot = sink
        .snapshot()
        .await
        .map_err(|e| Error::session(format!("snapshot sink: {e}")))?;
    // Stale-cursor detection: only fires when the caller
    // explicitly passed a `from` past the sink tail. A
    // missing or zero `from` is treated as "from the
    // start" — replay starts at index 1, the snapshot
    // frame carries the current `last_stream_index`,
    // and the cursor frame that follows carries the same.
    if from > 0 && from > snapshot.last_stream_index {
        return Ok(resume_cursor_truncated_response(
            from,
            snapshot.last_event_seq,
            snapshot.last_stream_index,
        ));
    }
    let replay = sink
        .read_from_index(from)
        .await
        .map_err(|e| Error::session(format!("read sink: {e}")))?;
    let stream = resume_stream_to_sse(
        rx,
        Arc::clone(&sink),
        snapshot.last_event_seq,
        snapshot.last_stream_index,
        replay,
    );
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

fn parse_resume_cursor(raw: Option<&str>) -> Result<Option<u64>, AppError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    raw.parse::<u64>().map(Some).map_err(|_| {
        AppError::new(
            StatusCode::BAD_REQUEST,
            Error::invalid_item(format!(
                "query parameter `from` must be an unsigned integer (got `{raw}`)"
            )),
        )
        .with_code("invalid_resume_cursor")
        .with_message(format!(
            "from must be an unsigned integer; got `{raw}`"
        ))
    })
}

/// Build the `410 Gone` response carrying the current
/// `last_event_seq` so the client can restart. Returns a
/// real `Response` (not an `AppError`) because the unified
/// error envelope does not surface per-field payloads and
/// the spec requires the cursor on the wire.
fn resume_cursor_truncated_response(
    from: u64,
    current_last_event_seq: u64,
    current_stream_index: u64,
) -> Response {
    use axum::Json;
    use serde_json::json;
    let body = json!({
        "code": "resume_cursor_truncated",
        "message": format!(
            "resume cursor {from} is past the current stream_index ({current_stream_index}); restart the stream"
        ),
        "current_last_event_seq": current_last_event_seq,
        "current_stream_index": current_stream_index,
    });
    (StatusCode::GONE, Json(body)).into_response()
}

/// `is_terminal` — true for events that close a stream
/// (the controller loop's terminal `System::SessionEnded`
/// frame). Resume-aware SSE streams emit a cursor frame
/// after the terminal message so the client observes the
/// close.
fn is_terminal(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::System(synthia::harness::SystemEvent::SessionEnded { .. })
    )
}

/// Resume-aware SSE stream.
///
/// Wire shape (all frames are SSE `event: <name>` with `data:
/// <json>`; `\n\n` is implicit at the end of each frame):
///
/// ```text
/// event: snapshot
/// data: {"last_event_seq": <u64>}
///
/// event: message
/// data: <replay event 1>
///
/// event: message
/// data: <replay event 2>
///
/// ... (one frame per replayed event, in chronological order)
///
/// event: cursor
/// data: {"last_event_seq": <u64>}
///
/// event: message
/// data: <live event>
///
/// ... (live tail continues until SessionEnded)
/// ```
///
/// The first `snapshot` frame carries the `last_event_seq`
/// captured before the replay so the client knows exactly
/// which cursor the prefix covers. The first `cursor` frame
/// is emitted unconditionally after the replay — even when
/// the replay is empty — so a client receiving a successful
/// `200` can always learn the current cursor without waiting
/// for a live event.
///
/// On the live tail, a `cursor` frame is also emitted every
/// `RESUME_CURSOR_BATCH` messages so a long-running run does
/// not strand a reconnecting client on a stale cursor.
const RESUME_CURSOR_BATCH: usize = 50;

/// Shared cursor frame builder.
fn cursor_frame(last_event_seq: u64, stream_index: u64) -> Event {
    let json = serde_json::json!({
        "last_event_seq": last_event_seq,
        "stream_index": stream_index,
    })
    .to_string();
    Event::default().event("cursor").data(json)
}

/// Snapshot frame builder (used once, at the head of the
/// replay). Mirrors `cursor_frame` today but the named
/// distinction lets the client tell the two apart.
fn snapshot_frame(last_event_seq: u64, stream_index: u64) -> Event {
    let json = serde_json::json!({
        "last_event_seq": last_event_seq,
        "stream_index": stream_index,
    })
    .to_string();
    Event::default().event("snapshot").data(json)
}

fn resume_stream_to_sse(
    rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    sink: Arc<dyn synthia::session::SessionSink>,
    last_event_seq: u64,
    stream_index: u64,
    replay: Vec<Value>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        // 1. Initial `snapshot` frame — carries the
        // `last_event_seq` the replay was carved from. A
        // client reconnecting with a cursor it had
        // previously observed can detect truncation by
        // comparing this with its own cursor.
        yield Ok(snapshot_frame(last_event_seq, stream_index));
        // 2. Replayed events, each as a `message` frame.
        // The JSON values stored in the sink are the same
        // `serde_json::Value` the live tail re-serialises,
        // so the bytes are byte-identical to what a
        // client observed from the original `append`.
        for value in replay {
            let json = serde_json::to_string(&value)
                .unwrap_or_else(|_| "{}".to_string());
            yield Ok(Event::default()
                .event("message")
                .data(json));
        }
        // 3. Post-replay cursor — guaranteed even when the
        // replay was empty so a "nothing to replay"
        // connection still learns the current cursor.
        yield Ok(cursor_frame(last_event_seq, stream_index));
        // 4. Live tail — broadcast events forwarded as
        // `message` frames. The cursor is re-published
        // every `RESUME_CURSOR_BATCH` messages so a
        // reconnecting client never has to wait for the
        // next batch boundary to learn its position.
        // Each cursor is refreshed against the live
        // `sink.snapshot()` so the seq reflects the latest
        // durable event, not a stale replay-time number.
        let mut rx = rx;
        let mut since_cursor = 0usize;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let terminal = is_terminal(&event);
                    let json = serde_json::to_string(&event)
                        .unwrap_or_else(|_| "{}".to_string());
                    yield Ok(Event::default()
                        .event("message")
                        .data(json));
                    since_cursor += 1;
                    if since_cursor >= RESUME_CURSOR_BATCH {
                        since_cursor = 0;
                        let live_seq = match sink.snapshot().await {
                            Ok(snap) => snap.last_event_seq,
                            Err(_) => last_event_seq,
                        };
                        yield Ok(cursor_frame(live_seq, live_seq));
                    }
                    if terminal {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

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
    // Recover the turn BEFORE resolving the controller.
    // `get_or_create_session_controller` eagerly creates the session
    // when it is unknown, so resolving first would make a miss —
    // the one case that must not have side effects — the very path
    // that creates a session.
    //
    // A miss is also NOT a cancel. `Cancel` is the same op the
    // `/cancel` route submits, so answering "nothing to replay"
    // with it would abort the session — a destructive side effect
    // from a read that simply found no user turn (a log whose
    // prompt was shadowed by a compaction `Replace`, a session
    // driven only by programmatic input, or a session created but
    // never written, whose log file does not exist yet). Report the
    // miss instead.
    let Some(parts) =
        read_last_user_turn(&state.session_manager, &user_id, &session_id)
            .await
    else {
        return Err(Error::not_found(
            "no user turn to regenerate in this session",
        )
        .into());
    };
    let controller = state
        .get_or_create_session_controller(&user_id, &session_id)
        .await?;
    let priority = 1u8;
    let agent_name = state.resolve_agent_name_for(None);
    controller
        .submit(SessionOp::Rerun {
            parts,
            agent_name,
            priority,
        })
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
    // The session arrives in the body, so it is caller-supplied and
    // reaches the sink's filesystem path — validate it exactly like a
    // path parameter.
    validate_resource_name(&req.session_id)?;
    let session_id = req.session_id;
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
///
/// `attachment_store` enables the `kind = "attachment_ref"`
/// resolution path: the hash is looked up through the R11 store
/// and the bytes are inlined as an `Image` part (replay-safe via
/// content addressing). Pass `None` in tests / when the store is
/// unavailable; unresolved refs are skipped with a warn log.
fn build_parts(
    req: &SendMessageRequest,
    attachment_store: Option<&synthia::attachment::AttachmentStore>,
) -> Vec<ContentPart> {
    let mut parts: Vec<ContentPart> = Vec::new();
    if !req.text.is_empty() {
        parts.push(text_part(&req.text));
    }
    for a in &req.attachments {
        let part = match a.kind.as_str() {
            "image" => image_part(a),
            "attachment_ref" => attachment_ref_part(a, attachment_store),
            "audio" => audio_part(a),
            "file" => file_part(a),
            "url" => url_part(a),
            _ => None,
        };
        if let Some(part) = part {
            parts.push(part);
        }
    }
    parts
}

/// Wrap plain prompt text in the wire-neutral `Text` variant.
fn text_part(s: &str) -> ContentPart {
    ContentPart::Text(synthia::provider::TextContent {
        text: s.to_string(),
        cache_control: None,
    })
}

/// `kind = "image"` — inline base64 or remote URL, both end up
/// as an `Image` part with `detail = Auto`.
fn image_part(a: &WireAttachment) -> Option<ContentPart> {
    let data = a.data_base64.as_ref().or(a.url.as_ref())?;
    Some(ContentPart::Image(synthia::provider::ImageContent {
        mime_type: a.mime_type.clone().unwrap_or_default(),
        data: data.clone(),
        detail: Some(synthia::provider::ImageDetail::Auto),
    }))
}

/// `kind = "attachment_ref"` — resolve the hash through the R11
/// store and inline the bytes as a base64 `Image` part. Returns
/// `None` (after logging) when the hash is missing, the store
/// is unavailable, or the lookup fails — the turn ships without
/// the attachment rather than failing the request.
fn attachment_ref_part(
    a: &WireAttachment,
    attachment_store: Option<&synthia::attachment::AttachmentStore>,
) -> Option<ContentPart> {
    let (Some(hash), Some(store)) = (&a.attachment_hash, attachment_store)
    else {
        tracing::warn!(
            hash = ?a.attachment_hash,
            "attachment_ref missing hash or store; skipping"
        );
        return None;
    };
    match store.read_base64_raw(hash) {
        Ok(bytes) => {
            Some(ContentPart::Image(synthia::provider::ImageContent {
                mime_type: a
                    .mime_type
                    .clone()
                    .unwrap_or_else(|| "image/png".to_string()),
                data: base64::engine::general_purpose::STANDARD.encode(&bytes),
                detail: Some(synthia::provider::ImageDetail::Auto),
            }))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                hash,
                "attachment_ref resolve failed; skipping"
            );
            None
        }
    }
}

/// `kind = "audio"` — inline base64 audio bytes. The container is left
/// unset so it is derived from the media type at the wire boundary
/// (see `AudioContent::format_label`): the composer accepts any
/// `audio/*`, so hardcoding `Wav` here would declare non-WAV bytes as a
/// WAV container and the provider rejects the mismatch.
fn audio_part(a: &WireAttachment) -> Option<ContentPart> {
    let b64 = a.data_base64.as_ref()?;
    Some(ContentPart::Audio(synthia::provider::types::AudioContent {
        mime_type: a.mime_type.clone().unwrap_or_default(),
        data: b64.clone(),
        format: None,
    }))
}

/// `kind = "file"` — generic binary file. The provider does not
/// expose a dedicated `RawContent`; binary files arrive as a
/// generic `ResourceLink` carrying the mime type and a base64
/// payload (wrapped in a `data:` URI). The agent runtime reads
/// the `mime_type` to decide how to consume the `data` field.
fn file_part(a: &WireAttachment) -> Option<ContentPart> {
    let b64 = a.data_base64.as_ref()?;
    Some(ContentPart::Resource(synthia::provider::ResourceLink {
        mime_type: a.mime_type.clone(),
        uri: format!(
            "data:{};base64,{}",
            a.mime_type.clone().unwrap_or_default(),
            b64
        ),
        name: a.filename.clone().unwrap_or_default(),
        title: None,
        description: None,
    }))
}

/// `kind = "url"` — reference an external resource by URL.
fn url_part(a: &WireAttachment) -> Option<ContentPart> {
    let url = a.url.as_ref()?;
    Some(ContentPart::Resource(synthia::provider::ResourceLink {
        mime_type: a.mime_type.clone(),
        uri: url.clone(),
        name: a.filename.clone().unwrap_or_default(),
        title: None,
        description: None,
    }))
}

/// Read the most recent user turn's parts from the on-disk session
/// log so the `regenerate` endpoint can replay the same payload.
///
/// Returns `None` for every "nothing to replay" condition, including
/// an unreadable log — a session whose `events.jsonl` does not exist
/// yet (created but never written) is a miss, not a server fault, and
/// letting the read error through would surface it as a `500`.
async fn read_last_user_turn(
    registry: &SessionRegistry,
    user_id: &str,
    session_id: &str,
) -> Option<Vec<ContentPart>> {
    let sink = registry.sink(user_id, session_id);
    match sink.read().await {
        Ok(events) => extract_last_user_parts(&events),
        Err(e) => {
            tracing::debug!(
                target: "synthia.session",
                session_id = %session_id,
                error = %e,
                "regenerate: session log unreadable; treating as no user turn"
            );
            None
        }
    }
}

fn extract_last_user_parts(events: &[Value]) -> Option<Vec<ContentPart>> {
    // Project through the shared raw-log fold rather than matching a
    // row shape by hand. That fold is the single decoder for every
    // envelope the sink has ever carried — the controller's
    // `UserInput` rows, the typed `user_message` family, the
    // lossless `Message` envelope, and the legacy `Model` parts —
    // and it honours a durable compaction checkpoint. A hand-rolled
    // matcher rots the moment the writer changes, which is exactly
    // what happened here: the previous version looked for
    // `{"role": …, "parts": […]}`, a shape no writer emits, so
    // `regenerate` found nothing, never submitted `Rerun`, and
    // fell through to cancelling the run.
    //
    // Only the textual component is recoverable (image and audio
    // bytes are deliberately not serialised into the sink), so a
    // regenerated multimodal turn replays its prompt text.
    for row in synthia::session::fold_log_surface(events)
        .messages
        .iter()
        .rev()
    {
        let Ok(message) = serde_json::from_value::<Message>(row.clone()) else {
            continue;
        };
        if message.role != Role::User {
            continue;
        }
        let parts: Vec<ContentPart> = message.content.iter().cloned().collect();
        if !parts.is_empty() {
            return Some(parts);
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
        ContentPart::Text(synthia::provider::TextContent {
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
                mime_type: Some("image/png".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let parts = build_parts(&req, None);
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn wire_attachment_attachment_hash_serde_round_trip() {
        // Round-trip an attachment_ref through serde. The chat
        // pipeline serialises the hash so the agent runtime can
        // resolve it through `AppState::attachment_store` later;
        // R12-3 wires the actual resolve step.
        let json = r#"{
            "kind": "attachment_ref",
            "mime_type": "image/png",
            "attachment_hash": "abc123def456"
        }"#;
        let a: WireAttachment = serde_json::from_str(json).unwrap();
        assert_eq!(a.kind, "attachment_ref");
        assert_eq!(a.attachment_hash.as_deref(), Some("abc123def456"));
        let back = serde_json::to_value(&a).unwrap();
        assert_eq!(back["attachment_hash"], "abc123def456");
        assert_eq!(back["mime_type"], "image/png");
    }

    #[test]
    fn build_parts_skips_empty_text() {
        let req = SendMessageRequest {
            text: String::new(),
            attachments: vec![WireAttachment {
                kind: "audio".into(),
                data_base64: Some("AAAA".into()),
                mime_type: Some("audio/wav".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let parts = build_parts(&req, None);
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], ContentPart::Audio(_)));
    }

    /// The controller's real user-prompt row is
    /// `{"type":"UserInput","data":{"text":…}}`. It MUST be
    /// recoverable, because it is the only shape the running system
    /// writes — the previous matcher looked for
    /// `{"role":…,"parts":[…]}` instead, found nothing in any real
    /// log, and silently turned every regenerate into a cancel.
    #[test]
    fn extract_last_user_parts_reads_the_controller_user_input_row() {
        let events = vec![
            json!({"type": "UserInput", "data": {"text": "first"}}),
            json!({"type": "Model", "data": {"type": "text", "text": "reply"}}),
            json!({"type": "UserInput", "data": {"text": "second"}}),
        ];
        let parts = extract_last_user_parts(&events).expect("a user turn");
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            ContentPart::Text(t) => assert_eq!(t.text, "second"),
            other => panic!("expected the text part, got {other:?}"),
        }
    }

    /// The lossless `Message` envelope (`SessionMemory::append`)
    /// decodes too, so a log written by that adapter regenerates.
    #[test]
    fn extract_last_user_parts_reads_the_message_envelope() {
        let events = vec![json!({
            "type": "Message",
            "data": serde_json::to_value(Message::user("from memory"))
                .unwrap(),
        })];
        let parts = extract_last_user_parts(&events).expect("a user turn");
        match &parts[0] {
            ContentPart::Text(t) => assert_eq!(t.text, "from memory"),
            other => panic!("expected the text part, got {other:?}"),
        }
    }

    /// The typed `user_message` row carries a multi-part content
    /// array; every part has to survive, not just the text.
    #[test]
    fn extract_last_user_parts_keeps_every_part_of_a_typed_row() {
        let events = vec![json!({
            "type": "user_message",
            "seq": 1,
            "data": {
                "role": "user",
                "content": [
                    {"type": "text", "text": "what is this?"},
                    {
                        "type": "image",
                        "data": "QUJD",
                        "mime_type": "image/png",
                        "detail": null,
                    }
                ],
            }
        })];
        let parts = extract_last_user_parts(&events).expect("a user turn");
        assert_eq!(parts.len(), 2, "got {parts:?}");
        assert!(matches!(&parts[1], ContentPart::Image(_)));
    }

    /// A log with no user turn must yield `None` — that is the signal
    /// `regenerate` uses to answer 202 without rerunning a turn that
    /// does not exist.
    #[test]
    fn extract_last_user_parts_returns_none_without_a_user_turn() {
        let assistant_only = vec![
            json!({"type": "Model", "data": {"type": "text", "text": "hi"}}),
        ];
        assert!(extract_last_user_parts(&assistant_only).is_none());
        assert!(extract_last_user_parts(&[]).is_none());
        // A user-role *tool* row is not a user turn.
        let tool = vec![json!({
            "type": "Model",
            "data": {
                "type": "tool_result",
                "tool_use_id": "c1",
                "content": [{"type": "text", "text": "out"}],
            }
        })];
        assert!(extract_last_user_parts(&tool).is_none());
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
        let parts = build_parts(&req, None);
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
                mime_type: Some("image/jpeg".into()),
                filename: Some("cat.jpg".into()),
                ..Default::default()
            }],
            agent_name: None,
            model: None,
        };
        let parts = build_parts(&req, None);
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

    /// R13-1: `kind = "attachment_ref"` resolves the hash through
    /// the R11 attachment store and inlines the bytes as an
    /// `Image` part — the full hash→store→bytes→wire round trip.
    #[test]
    fn attachment_ref_resolves_through_store_into_image_part() {
        // 1x1 transparent PNG (same fixture as the attachment
        // crate's tests).
        let png: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00,
            0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89,
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63,
            0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x6F, 0x32,
            0x4D, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
            0x42, 0x60, 0x82,
        ];
        let store = synthia::attachment::AttachmentStore::default();
        let saved = store.save_image(&png, "image/png").unwrap();

        let req = SendMessageRequest {
            text: "what is this?".into(),
            attachments: vec![WireAttachment {
                kind: "attachment_ref".into(),
                attachment_hash: Some(saved.hash.clone()),
                mime_type: Some("image/png".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let parts = build_parts(&req, Some(&store));
        // Text + resolved Image.
        assert_eq!(parts.len(), 2);
        match &parts[1] {
            ContentPart::Image(img) => {
                assert_eq!(img.mime_type, "image/png");
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(&img.data)
                    .unwrap();
                assert_eq!(decoded, png, "bytes must round-trip losslessly");
            }
            other => panic!("expected Image part, got {other:?}"),
        }
    }

    /// R13-1: an unresolvable hash (never uploaded) is skipped —
    /// the text part still ships so the turn isn't lost.
    #[test]
    fn attachment_ref_unknown_hash_skips_not_fails() {
        let store = synthia::attachment::AttachmentStore::default();
        let req = SendMessageRequest {
            text: "hello".into(),
            attachments: vec![WireAttachment {
                kind: "attachment_ref".into(),
                attachment_hash: Some("deadbeef".repeat(8)),
                ..Default::default()
            }],
            ..Default::default()
        };
        let parts = build_parts(&req, Some(&store));
        // Only the text part; the dead ref was skipped.
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], ContentPart::Text(_)));
    }
}
