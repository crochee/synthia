//! The Anthropic Messages-compatible surface: `POST /v1/messages`.
//!
//! An Anthropic-protocol client — the Anthropic SDKs, Claude Code,
//! editor plugins — can point its `base_url` at this server and talk to
//! it. The route is additive: every `/api/v1/chat/*` endpoint keeps its
//! existing wire shape, and this module shares their run machinery
//! rather than growing a second one.
//!
//! # Contract
//!
//! - `POST /v1/messages`
//!   Body: the Messages request (`model`, `max_tokens`, `messages`,
//!   optional `system` / `tools` / `stream`). Unknown fields are
//!   ignored, exactly as an Anthropic-compatible endpoint should.
//!   Response: `200` with `{id, type:"message", role:"assistant",
//!   model, content:[blocks], stop_reason, stop_sequence, usage}`, or —
//!   with `"stream": true` — an SSE stream of
//!   `message_start` → `content_block_start` → `content_block_delta`* →
//!   `content_block_stop` (per block) → `message_delta` →
//!   `message_stop`.
//! - Every failure carries the protocol's envelope,
//!   `{"type":"error","error":{"type":…,"message":…}}`, never the
//!   application's `{code,message}` one. That includes the `401` the
//!   auth layer produces for this path.
//!
//! # Auth
//!
//! The route sits on the **protected** router and the shared auth
//! middleware accepts `x-api-key` as an alternative to
//! `Authorization: Bearer` (see
//! [`crate::middleware::auth::AuthMiddleware`]). The decision, and why
//! the endpoint is not mounted public, is documented there; the
//! observable rule is:
//!
//! - `SYNTHIA_API_KEY` set → `x-api-key` (or `Authorization: Bearer`)
//!   must match, and the resolved `user_id` namespaces the session;
//! - `SYNTHIA_API_KEY` unset → the server's existing
//!   "unconfigured ⇒ default user" behaviour, so a local client needs no
//!   credentials.
//!
//! `anthropic-version` is accepted and not validated: the goal of this
//! endpoint is compatibility, and every Anthropic client sends it.
//!
//! # One request, one session — or a pinned one
//!
//! The protocol is stateless — the client resends the whole
//! conversation — while the runtime is a session whose transcript is
//! durable. How the two are bridged depends on whether the caller names
//! a session, which is what `metadata.synthia_session_id` is for.
//!
//! ## No session named: one request, one session
//!
//! 1. A fresh session is minted per request
//!    (`anthropic-<uuid>`), so the client's history is the *only*
//!    history the run sees and a client that edits its own history is
//!    honoured rather than shadowed by a stale transcript;
//! 2. the request's earlier messages are written into that session's
//!    sink as `{"type":"Message"}` rows — the fold's lossless envelope
//!    (`synthia::session::fold_log_surface`), which the run task projects
//!    back into `AgentInput::history` with their real roles, including
//!    tool calls and results;
//! 3. the final user turn is dispatched through the same
//!    [`SessionOp::PromptMulti`] the chat surface uses, and the response
//!    is folded from the same event broadcast
//!    (`routes::chat::stream_messages` reads it the same way);
//! 4. when the turn ends the session is closed and evicted, so a
//!    stateless client cannot grow the controller cache (nothing else
//!    ever evicts it).
//!
//! ## `metadata.synthia_session_id`: a pinned session
//!
//! Synthia's own web client is not a stateless Anthropic client: it
//! shows one conversation, persists its session id, and addresses that
//! id for its sessions list, its feedback, its cancel and its
//! regenerate. Minting a session per request would give every turn a new
//! one and break all of that. So a request may name the session it is
//! continuing, in `metadata` — a field the protocol defines as a
//! free-form object with exactly one reserved key (`user_id`), which
//! makes a namespaced `synthia_session_id` an extension no real
//! Anthropic client can collide with, and one none of them ever sends.
//! Its sibling `synthia_agent_name` names the agent that answers, so the
//! client's agent choice is a dispatch input rather than a label.
//!
//! A pinned request differs in exactly three ways, each forced by "this
//! session outlives the request":
//!
//! - the id is **validated** with the same [`validate_resource_name`]
//!   gate every path parameter passes — it reaches the sink's
//!   filesystem path (`<root>/<user_id>/<session_id>/events.jsonl`), and
//!   an unvalidated id there is a path traversal. A malformed id answers
//!   `400 invalid_request_error`; it is never silently replaced by a
//!   minted one, because a caller that names a session and gets a
//!   different one cannot tell.
//! - an id the server has not seen is **adopted**: it becomes a session
//!   under the request's own resolved user, exactly as
//!   `/api/v1/chat/*` auto-creates one. Requiring a pre-created session
//!   would break the first turn of every new conversation.
//! - history is **not re-seeded** once the session holds rows: the
//!   transcript *is* the conversation, and replaying the client's view
//!   of it on every turn would duplicate it — see [`seed_history`].
//! - the session is **not closed or evicted** when the turn ends. It is
//!   left to the same `DEFAULT_IDLE_TIMEOUT` reclaim every
//!   `/api/v1/chat/*` session gets, so the next turn finds a live
//!   controller, and so do feedback, cancel, regenerate and the
//!   session-detail page.
//!
//! The same gate turns on tool blocks: a pinned request is one of ours,
//! and one of ours renders `工具 · <name>` blocks with 请求 / 结果
//! halves. An external client must never be handed a call to answer, so
//! it keeps seeing none — see [`blocks`].
//!
//! # Deliberate limitations
//!
//! - **`tools` are converted, not injected.** The run's tool surface is
//!   the server's own (registry + `[tools]` config); the
//!   session-controller seam has no per-dispatch tool injection point,
//!   so a tool a client defines is parsed and validated but the model
//!   can only call the tools this server has. It is also why the
//!   response never contains an executed `tool_use` block: the agent
//!   runs its own tools, so a client is never asked to answer a call.
//! - **Sampling parameters are not plumbed.** `max_tokens` is validated
//!   and otherwise ignored; `temperature` / `top_p` / `stop_sequences`
//!   are not even parsed. The harness deliberately leaves sampling
//!   parameters to the deployment's model configuration, and it has no
//!   per-dispatch override.
//! - **`model` is echoed, not switched.** The run resolves its
//!   provider/model the same way `/api/v1/chat/*` does.
//! - **Audio has no block** in this protocol, so an `audio/*`
//!   attachment cannot ride this endpoint at all. `/api/v1/chat/*` is
//!   the wire that carries one (`kind: "audio"`); here the client is
//!   the only party that can keep one out of a request.
//! - **`count_tokens` is not implemented** (no such route is mounted;
//!   it falls through to the router's 404).
//! - **Assistant prefills are rejected**: the run always needs a user
//!   turn, so a final `assistant` message returns a `400`.
//!
//! # File attachments
//!
//! A `document` block is the protocol's file block — a PDF, or a plain
//! text or Markdown file. The canonical model has no document part, so
//! the block projects onto the same generic `ContentPart::Resource` the
//! old `/api/v1/chat/*` wire's `kind: "file"` attachment produced: an
//! inline source becomes a `data:<media_type>;base64,<payload>` URI and
//! a `url` source keeps its URL. A transcript replayed through either
//! endpoint is therefore identical — see [`convert::document_part`].

use std::{convert::Infallible, sync::Arc};

use axum::{
    Extension,
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse,
        Response,
        Sse,
        sse::{Event, KeepAlive},
    },
};
use futures::Stream;
use serde_json::json;
use synthia::{
    harness::{AgentEvent, SessionEndReason, SystemEvent},
    provider::{Message, traits::ModelProvider},
    session::SessionSink,
};
use tokio::sync::broadcast;
use uuid::Uuid;

use self::{
    blocks::Turn,
    convert::CanonicalRequest,
    wire::{ErrorEnvelope, MessagesRequest, Metadata, error_type},
};
use crate::{
    api::validate_resource_name,
    middleware::auth::RequestUserId,
    session::{
        EPHEMERAL_SESSION_PREFIX,
        controller::{PinnedProvider, SessionController, SessionOp},
    },
    state::AppState,
};

mod blocks;
mod convert;
mod wire;

/// Label on the leading turn that carries the client's `system` text.
///
/// The text cannot become a `Role::System` message (the harness
/// assembles the agent's own system prompt ahead of `history`, and the
/// Anthropic adapter promotes only the first one — see
/// [`convert`]), so it travels as the first user turn, labelled so the
/// model can tell it apart from the user's question.
const SYSTEM_LABEL: &str = "System instructions from the client:";

// ---------------------------------------------------------------------------
// POST /v1/messages
// ---------------------------------------------------------------------------

/// `POST /v1/messages` — the Anthropic Messages endpoint.
///
/// The body is taken as raw [`Bytes`] rather than a `Json<T>`
/// extractor: an extractor rejection would answer with axum's own error
/// body, and this route must answer with the protocol's envelope.
pub(crate) async fn create_message(
    State(state): State<Arc<AppState>>,
    user: Option<Extension<RequestUserId>>,
    body: Bytes,
) -> Response {
    let request: MessagesRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                error_type::INVALID_REQUEST,
                format!("invalid request body: {error}"),
            );
        }
    };
    let canonical = match convert::to_canonical(&request) {
        Ok(canonical) => canonical,
        Err(error) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                error_type::INVALID_REQUEST,
                error.to_string(),
            );
        }
    };
    let user_id = user
        .map(|Extension(id)| id.0)
        .unwrap_or_else(|| state.default_user_id().to_string());
    // Which session this turn belongs to and which agent answers it —
    // resolved (and, for a named id, validated) before anything touches
    // the sink.
    let target = match resolve_target(request.metadata.as_ref()) {
        Ok(target) => target,
        Err(message) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                error_type::INVALID_REQUEST,
                message,
            );
        }
    };
    // The turn's provider: the client names a model on every request
    // (the protocol requires it), so the selection is resolved before
    // anything else happens and a selection this deployment cannot serve
    // is refused up front — never silently answered by a different
    // model. The same `Arc` runs the turn (it is pinned to the dispatch
    // below) *and* names it in the response.
    let provider = match state.resolve_provider(Some(&request.model)) {
        Ok(provider) => provider,
        Err(error) => {
            return error_response(
                error.status(),
                model_error_type(error.status()),
                error.to_string(),
            );
        }
    };
    let streaming = request.stream.unwrap_or(false);
    match begin_turn(
        &state,
        &user_id,
        &request.model,
        canonical,
        target,
        provider,
    )
    .await
    {
        Ok(turn) if streaming => streaming_response(turn),
        Ok(turn) => complete_response(turn).await,
        Err(error) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error_type::API,
            format!("could not start the agent run: {error}"),
        ),
    }
}

/// Which protocol error type a model-selection failure carries.
///
/// The two are different faults and a client acts on them differently:
/// a selection this deployment does not know is the caller's mistake
/// (`invalid_request_error`, a `400`), while a provider that *is*
/// configured but could not be built at boot — a missing key, an
/// adapter that is not compiled in — is the server's
/// (`api_error`, a `503`).
fn model_error_type(status: StatusCode) -> &'static str {
    if status == StatusCode::SERVICE_UNAVAILABLE {
        error_type::API
    } else {
        error_type::INVALID_REQUEST
    }
}

/// How a request's turn is dispatched: the session it runs in, and the
/// agent that answers it.
struct Target {
    /// `None` when the client named no session — mint one, and own its
    /// whole lifetime. `Some` for a pinned id, which has already passed
    /// [`validate_resource_name`] by the time it is here.
    session: Option<String>,
    /// The client's `metadata.synthia_agent_name`, handed to the same
    /// `explicit > configured default > first registered` ladder every
    /// dispatch resolves through. `None` resolves the default, exactly
    /// as a request with no `metadata` does.
    agent_name: Option<String>,
}

impl Target {
    /// Whether the request's client is one of ours — the gate for the
    /// tool blocks an external client must never be handed (see
    /// [`blocks`]).
    fn is_pinned(&self) -> bool {
        self.session.is_some()
    }
}

/// Decide which session the request's turn runs in, and which agent
/// answers it.
///
/// The only things this reads out of `metadata` are the two namespaced
/// keys [`Metadata`] models; everything else in the object (the
/// protocol's own `user_id` included) is ignored, as it always has been.
///
/// A named session is **adopted**: an id this server has never seen
/// becomes a session under the request's own user, which is what makes
/// the web client's first turn of a new conversation work without a
/// separate create call. What is never done is substituting a *different*
/// id — a caller that names a session and silently gets another would
/// keep appending to a conversation nobody reads.
///
/// # Errors
///
/// The reason a named id was refused, for the `400` the handler answers
/// with: a malformed one is the one case, and it is refused because it
/// reaches the sink's filesystem path.
fn resolve_target(metadata: Option<&Metadata>) -> Result<Target, String> {
    let Some(metadata) = metadata else {
        return Ok(Target {
            session: None,
            agent_name: None,
        });
    };
    let session = match metadata.synthia_session_id.as_deref() {
        Some(session_id) => {
            validate_resource_name(session_id).map_err(|error| {
                format!(
                    "metadata.synthia_session_id is not a usable session \
                     id: {error}"
                )
            })?;
            Some(session_id.to_string())
        }
        None => None,
    };
    Ok(Target {
        session,
        agent_name: metadata.synthia_agent_name.clone(),
    })
}

/// A run that has been dispatched but has not finished.
struct StartedTurn {
    controller: Arc<SessionController>,
    events: broadcast::Receiver<AgentEvent>,
    accumulator: Turn,
    /// `Some` only for a session this request minted. Dropped with the
    /// turn: evicts the session when the response stops being polled,
    /// including a client that vanishes mid-stream.
    ///
    /// `None` for a pinned session, which outlives the request — see
    /// [`close_turn`].
    ephemeral: Option<SessionSlot>,
}

/// Mint a session or continue the pinned one, replay the request's
/// history into it, and dispatch the final user turn through the shared
/// controller path.
async fn begin_turn(
    state: &Arc<AppState>,
    user_id: &str,
    model: &str,
    canonical: CanonicalRequest,
    target: Target,
    provider: Arc<dyn ModelProvider>,
) -> anyhow::Result<StartedTurn> {
    let CanonicalRequest {
        system,
        history,
        turn,
        tools,
    } = canonical;
    tracing::debug!(
        target: "synthia.server",
        tool_definitions = tools.len(),
        "anthropic request: tools are parsed but the run uses the server's own tool surface"
    );
    let ephemeral = !target.is_pinned();
    let session_id = match target.session {
        None => {
            let session_id = format!(
                "{EPHEMERAL_SESSION_PREFIX}{}",
                Uuid::new_v4().simple()
            );
            validate_resource_name(&session_id).map_err(|error| {
                anyhow::anyhow!("generated session id rejected: {error}")
            })?;
            session_id
        }
        Some(session_id) => session_id,
    };
    // A pinned session's transcript is the conversation: seeding the
    // client's view of it again would duplicate every earlier turn. Its
    // first turn is the exception — the session has no rows yet, and the
    // `system` and history that arrived with it belong in the log.
    if ephemeral || !session_holds_rows(state, user_id, &session_id).await {
        seed_history(state, user_id, &session_id, system, history).await?;
    }
    let controller = state
        .get_or_create_session_controller_with_parent(
            user_id,
            &session_id,
            None,
            None,
        )
        .await?;
    // Subscribe before dispatching: the broadcaster only replays
    // nothing, so a subscriber that arrives after the run starts would
    // miss the opening frames.
    let events = controller.subscribe();
    controller
        .submit_with_provider(
            SessionOp::PromptMulti {
                parts: turn,
                agent_name: state
                    .resolve_agent_name_for(target.agent_name.as_deref()),
                priority: 1,
            },
            // The pin is consumed by the run this op starts, so the
            // model this request named is the model that answers it.
            Some(PinnedProvider::new(provider)),
        )
        .await?;
    Ok(StartedTurn {
        controller,
        events,
        accumulator: Turn::new(
            format!("msg_{}", Uuid::new_v4().simple()),
            model,
            !ephemeral,
        ),
        // `then`, not `then_some`: dropping a slot evicts the key, so a
        // pinned turn must never build one in the first place.
        ephemeral: ephemeral.then(|| SessionSlot {
            state: Arc::clone(state),
            key: (user_id.to_string(), session_id),
        }),
    })
}

/// Whether a session's durable log already holds rows.
///
/// A read failure counts as "no rows": the caller only uses this to
/// decide whether to seed, and seeding a log that cannot be read is the
/// same as seeding an empty one (the run's own sink read will surface
/// the failure).
async fn session_holds_rows(
    state: &AppState,
    user_id: &str,
    session_id: &str,
) -> bool {
    state
        .session_manager
        .sink(user_id, session_id)
        .read()
        .await
        .is_ok_and(|rows| !rows.is_empty())
}

/// Write the request's `system` and earlier turns into a session's sink,
/// so the run projects them back as real history.
///
/// Two sessions are the same conversation only if the client says so,
/// and a stateless client says so by resending the transcript — which is
/// why the rows are written on every request for a session this request
/// minted. A **pinned** session is the other case: its transcript is the
/// conversation, so it is seeded only while it has no rows yet (its
/// first turn, where the client's `system` and any history it sent
/// belong in the log). Seeding it on every turn would append the
/// conversation to itself — the same earlier turns on every request, and
/// a duplicate of each in the model's context. The caller decides; see
/// [`begin_turn`].
async fn seed_history(
    state: &Arc<AppState>,
    user_id: &str,
    session_id: &str,
    system: Option<String>,
    history: Vec<Message>,
) -> anyhow::Result<()> {
    let sink = state.session_manager.sink(user_id, session_id);
    if let Some(system) = system {
        append_message(
            &sink,
            Message::user(format!("{SYSTEM_LABEL}\n{system}")),
        )
        .await?;
    }
    for message in &history {
        append_message(&sink, message.clone()).await?;
    }
    Ok(())
}

/// Append one canonical message as the fold's lossless `Message`
/// envelope.
async fn append_message(
    sink: &Arc<dyn SessionSink>,
    message: Message,
) -> anyhow::Result<()> {
    let row = json!({ "type": "Message", "data": message });
    sink.append(&row)
        .await
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("seed session history: {error}"))
}

/// The streaming response: the protocol's frame sequence, driven by the
/// session's event broadcast.
fn streaming_response(turn: StartedTurn) -> Response {
    let stream = turn_frames(turn);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Pump the turn's frames until the run's terminal event.
fn turn_frames(
    turn: StartedTurn,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let mut turn = turn;
        yield Ok(turn.accumulator.start().event());
        loop {
            match turn.events.recv().await {
                Ok(event) => {
                    let terminal = is_terminal(&event);
                    for frame in turn.accumulator.on_event(&event) {
                        yield Ok(frame.event());
                    }
                    if terminal {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        target: "synthia.server",
                        skipped,
                        "anthropic stream lagged behind the session broadcast"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
        for frame in turn.accumulator.finish() {
            yield Ok(frame.event());
        }
        close_turn(&turn).await;
    }
}

/// The non-streaming response: fold the run to its terminal event and
/// serialise the assembled message.
///
/// A run that failed after the request was accepted answers with the
/// protocol's `api_error`, so a client never receives a `200` for a
/// turn the agent never completed.
async fn complete_response(turn: StartedTurn) -> Response {
    let mut turn = turn;
    loop {
        match turn.events.recv().await {
            Ok(event) => {
                let terminal = is_terminal(&event);
                turn.accumulator.on_event(&event);
                if terminal {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    target: "synthia.server",
                    skipped,
                    "anthropic run lagged behind the session broadcast"
                );
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    turn.accumulator.finish();
    let failure = match turn.accumulator.end_reason() {
        Some(SessionEndReason::Error(message)) => Some(message.clone()),
        _ => None,
    };
    let message = turn.accumulator.message();
    close_turn(&turn).await;
    match failure {
        Some(message) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error_type::API,
            message,
        ),
        None => axum::Json(message).into_response(),
    }
}

/// End the run's session, for a session this request minted.
///
/// Closing finalises the durable log immediately instead of at the idle
/// timeout, and is the right thing for an `anthropic-*` session nothing
/// else will ever address again.
///
/// A **pinned** session gets the opposite treatment: it is left open, to
/// be reclaimed by the same `DEFAULT_IDLE_TIMEOUT` every
/// `/api/v1/chat/*` session is. Closing it would leave the next turn —
/// and feedback, cancel, regenerate and `GET /api/v1/sessions/{id}` —
/// addressing a controller whose sink no longer accepts appends.
async fn close_turn(turn: &StartedTurn) {
    if turn.ephemeral.is_none() {
        return;
    }
    if let Err(error) = turn.controller.close().await {
        tracing::warn!(
            target: "synthia.server",
            error = %error,
            "anthropic session close failed; the idle timeout will reclaim it"
        );
    }
}

/// The terminal event of a run.
fn is_terminal(event: &AgentEvent) -> bool {
    matches!(event, AgentEvent::System(SystemEvent::SessionEnded { .. }))
}

/// Build the protocol's error response.
fn error_response(
    status: StatusCode,
    kind: &'static str,
    message: impl Into<String>,
) -> Response {
    (status, axum::Json(ErrorEnvelope::new(kind, message))).into_response()
}

/// Evicts a minted session from the controller cache when the response
/// stops being polled.
///
/// The normal path closes the controller first; this covers the
/// abandoned path — a client that disconnects mid-turn drops the
/// response future and no handler code runs again. Nothing else ever
/// evicts `AppState::active_sessions`, so without this every abandoned
/// request would pin a dead controller for the process's lifetime.
///
/// Only ever built for an ephemeral session: a pinned one is reachable
/// between turns, so [`close_turn`] never closes it and this is never
/// constructed for it.
struct SessionSlot {
    state: Arc<AppState>,
    key: (String, String),
}

impl Drop for SessionSlot {
    fn drop(&mut self) {
        self.state.active_sessions.remove(&self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated session ids must satisfy the resource-name rule
    /// the sink's path construction enforces, or every request would
    /// fail at `validate_resource_name`.
    #[test]
    fn generated_session_ids_are_valid_resource_names() {
        for _ in 0..8 {
            let id = format!(
                "{EPHEMERAL_SESSION_PREFIX}{}",
                Uuid::new_v4().simple()
            );
            validate_resource_name(&id).expect("generated id must validate");
        }
    }

    /// The error envelope is the protocol's, not the application's.
    #[test]
    fn error_envelope_uses_the_anthropic_shape() {
        let envelope =
            ErrorEnvelope::new(error_type::INVALID_REQUEST, "bad body");
        let json: serde_json::Value =
            serde_json::from_str(&envelope.body()).unwrap();
        assert_eq!(json["type"], "error");
        assert_eq!(json["error"]["type"], "invalid_request_error");
        assert_eq!(json["error"]["message"], "bad body");
        assert!(
            json.get("code").is_none(),
            "the application envelope must not leak: {json}"
        );
    }

    /// Only `SessionEnded` terminates a run's stream.
    #[test]
    fn only_session_ended_is_terminal() {
        assert!(is_terminal(&AgentEvent::System(
            SystemEvent::SessionEnded {
                reason: synthia::harness::SessionEndReason::Completed,
            }
        )));
        assert!(!is_terminal(&AgentEvent::text_delta("more")));
        assert!(!is_terminal(&AgentEvent::System(
            SystemEvent::SessionStarted {
                session_id: "s".to_string(),
            }
        )));
    }
}
