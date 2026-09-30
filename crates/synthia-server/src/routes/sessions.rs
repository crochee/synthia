//! Session management handlers.
//!
//! [`list_sessions`] merges the in-memory
//! [`synthia::session::manager::SessionRegistry`] with the durable
//! JSONL store: the registry only holds what this process has opened,
//! so on its own it hides every conversation recorded by an earlier
//! run. The endpoint shape is intentionally aligned with the chat
//! surface (`/api/v1/chat/sessions`) so callers can reuse the same
//! cursor pagination and filter semantics; only the URL path is
//! different (`/api/v1/sessions` is the read-optimised management
//! listing used by the `SessionsPage`, `/api/v1/chat/sessions` is the
//! chat surface used by `ChatPage`).
//!
//! Being a management view, the listing omits the throwaway sessions
//! the Messages endpoint mints for unbound requests — one
//! `anthropic-<uuid>` per Cursor / Claude Code / SDK turn, which
//! would otherwise bury the user's own conversations. The namespace
//! is `session::EPHEMERAL_SESSION_PREFIX` and the membership test is
//! "the id starts with that prefix", separator included, so an id
//! that only shares the letters (`anthropicx-1`) is listed normally.
//! A minted session stays durably addressable: [`get_session`] serves
//! one by id, and session search still indexes it.
//!
//! [`get_session`] fetches a single session's events directly from
//! the session sink — falling back to the transcript on disk when the
//! registry has no record of the id — so the frontend's
//! `SessionDetailPage` can render the full JSONL transcript.

// Allow `result_large_err`: `parse_status_filter` returns the
// un-boxed `synthia::core::Error` (≥128 bytes once the RFC-0977
// common fields are counted) — same accepted trade-off as
// `api/v1/cursor.rs` and `api/v1/validation.rs`; boxing would
// force `.map_err(|e| *e)` at every call site.
#![allow(clippy::result_large_err)]

use std::{
    collections::HashSet,
    io::{BufRead as _, BufReader},
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse as _, Response},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use synthia::{core::Error, session::is_metadata_header_row};
use uuid::Uuid;

use super::helpers::paginate;
use crate::{
    api::{
        AppError,
        AppJson,
        AppPath,
        AppQuery,
        List,
        SessionPageQuery,
        resolve_page,
        validate_resource_name,
        validate_sort,
    },
    session::{
        EPHEMERAL_SESSION_PREFIX,
        controller::{SessionController, SessionState},
    },
    state::AppState,
};

/// Sortable fields for the sessions list endpoint. The historical
/// `updated_at` field is no longer produced (the session sink has
/// no notion of update timestamps), so we accept only `created_at`
/// and `status` and silently drop `updated_at` if a client sends
/// it.
const SESSION_SORT_WHITELIST: &[&str] = &["created_at", "status"];

/// Frontend-facing session summary.
#[derive(Serialize)]
pub struct SessionSummary {
    pub id: String,
    /// Static string slice — the closed set of 9 status labels.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// Present when this session was opened by a delegated
    /// sub-agent (the `task` tool): the session the delegation
    /// ran in. A first-class session either way — the field only
    /// carries provenance, never privileges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    /// The registry name of the peer agent that ran this session
    /// (sub-agent sessions only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

/// Detailed session view including history. The `history`
/// field is a JSON-encoded array of session events (one
/// entry per agent / system frame persisted to the session
/// sink). The frontend's `SessionDetailPage` reads the entries
/// individually and runs them through the same renderer the
/// live `ChatPage` uses.
#[derive(Serialize)]
pub struct SessionDetail {
    pub id: String,
    pub status: &'static str,
    pub context_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    pub history: Vec<Value>,
    /// Legacy field — kept as an empty array so the
    /// SessionDetailPage doesn't have to special-case undefined.
    /// The modern equivalents (`ContentPart::Raw`,
    /// `ContentPart::Resource`) are inlined into `history`.
    pub artifacts: Vec<Value>,
    /// Sub-agent provenance — see [`SessionSummary::parent_session_id`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

/// Validated status filter labels. Rejects unknown labels so a
/// typo doesn't silently return every session.
fn parse_status_filter(status: &str) -> Result<&'static str, Error> {
    match status {
        "unspecified" => Ok("unspecified"),
        "submitted" => Ok("submitted"),
        "working" => Ok("working"),
        "completed" => Ok("completed"),
        "failed" => Ok("failed"),
        "canceled" => Ok("canceled"),
        "input_required" => Ok("input_required"),
        "rejected" => Ok("rejected"),
        "auth_required" => Ok("auth_required"),
        _ => Err(Error::invalid_item(format!("status filter '{status}'"))),
    }
}
/// `true` when a row survives the caller's `status` / `context_id`
/// filters. Both filters apply to every row regardless of whether the
/// session came from the registry or only from disk, so the two listing
/// loops share this one predicate.
///
/// `context_id` is currently identical to `id` (sessions have no
/// separate context-id column), so the filter is a strict equality
/// match against the session id — what a client narrowing the list
/// would expect.
fn selected(
    id: &str,
    status: &str,
    status_filter: Option<&str>,
    context_id_filter: Option<&str>,
) -> bool {
    if let Some(filter) = status_filter
        && filter != status
    {
        return false;
    }
    if let Some(needle) = context_id_filter
        && id != needle
    {
        return false;
    }
    true
}

/// `true` when `id` names a session in the Messages endpoint's
/// unbound-request namespace (see [`EPHEMERAL_SESSION_PREFIX`]).
///
/// Such a session is skipped by [`list_sessions`] only — at both of
/// its sources: the registered loop, and
/// [`scan_unregistered_transcripts`], which drops the directories
/// before opening their logs. It is not hidden anywhere else:
/// [`get_session`], `list_session_events`, `session_status` and
/// session search all still address it, because a caller that names
/// one of these ids has asked for it on purpose — this predicate
/// answers "does this id belong to the throwaway namespace", not "may
/// this session be seen".
///
/// The prefix carries its own separator, so membership is exact: a
/// session named `anthropicx-1` starts with the same letters but is
/// not in the namespace and is listed like any other.
fn is_ephemeral_session(id: &str) -> bool {
    id.starts_with(EPHEMERAL_SESSION_PREFIX)
}

/// Extracted separately from [`infer_status`] so the inference
/// itself stays unit-testable without spawning a real controller.
fn live_session_state(
    active_sessions: &DashMap<(String, String), Arc<SessionController>>,
    user_id: &str,
    session_id: &str,
) -> Option<SessionState> {
    active_sessions
        .get(&(user_id.to_string(), session_id.to_string()))
        .map(|ctrl| ctrl.state())
}

/// Inferred session status.
///
/// A session is "working" iff a live controller exists for it and
/// is mid-run. The durable sink alone cannot answer this:
/// `SessionEnded` / `SessionCanceled` / `SessionFailed` are
/// ephemeral broadcast-only events (per the
/// `event-durability-classification` spec) and are never persisted
/// to the JSONL, so a sink-tail scan can never observe a finished
/// run — it would report every non-empty session as "working"
/// forever.
///
/// With no live run: an empty sink is "unspecified"; a non-empty
/// sink has finished at least one run, so "completed" (or
/// "canceled" when the controller's last run was cancelled).
///
/// Also returns the sink's **last** event timestamp, which is the
/// listing's historical `created_at` for a session this process has
/// opened.
async fn infer_status(
    live_state: Option<SessionState>,
    sink: &dyn synthia::session::SessionSink,
) -> (&'static str, Option<DateTime<Utc>>) {
    let events = match sink.read().await {
        Ok(events) => events,
        Err(_) => return ("unspecified", None),
    };
    let ts = events
        .iter()
        .rev()
        .find_map(|ev| parse_rfc3339(ev.get("ts").and_then(|t| t.as_str())));
    (infer_status_from_parts(live_state, !events.is_empty()), ts)
}

/// The status rules of [`infer_status`], over the facts a caller has
/// already established.
///
/// Split out so a session known only from its durable transcript —
/// which has no sink to read through — is classified by the same
/// rules rather than re-deriving them.
fn infer_status_from_parts(
    live_state: Option<SessionState>,
    has_events: bool,
) -> &'static str {
    if matches!(live_state, Some(SessionState::Running)) {
        return "working";
    }
    if !has_events {
        return "unspecified";
    }
    match live_state {
        Some(SessionState::Cancelled) => "canceled",
        _ => "completed",
    }
}

fn parse_rfc3339(raw: Option<&str>) -> Option<DateTime<Utc>> {
    raw.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    })
}

/// `(parent_session_id, agent_name)` when a session's log opens
/// with the sub-agent router's `subagent_enter` header row — the
/// marker that this session is a delegated child. Reading only the
/// first event keeps both callers (the listing's two loops and the
/// detail view) cheap on long logs.
fn subagent_attribution(events: &[Value]) -> (Option<String>, Option<String>) {
    let Some(first) = events.first() else {
        return (None, None);
    };
    if first.get("type").and_then(Value::as_str) != Some("subagent_enter") {
        return (None, None);
    }
    let parent = first
        .pointer("/data/parent_session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let agent = first
        .pointer("/data/agent_name")
        .and_then(Value::as_str)
        .map(str::to_string);
    (parent, agent)
}
/// `<sessions_root>/<user_id>/<session_id>/events.jsonl`.
///
/// Refuses anything that is not a single directory entry, so a
/// hand-built path can never climb out of the sessions root:
/// `session_id` must match the resource-name regex the route path
/// parameter is already validated against (no `.`, `..`, `/` or `\`),
/// and `user_id` must be one path component.
fn transcript_path(
    sessions_root: &Path,
    user_id: &str,
    session_id: &str,
) -> Option<PathBuf> {
    if !is_path_component(user_id) {
        return None;
    }
    validate_resource_name(session_id).ok()?;
    Some(
        sessions_root
            .join(user_id)
            .join(session_id)
            .join("events.jsonl"),
    )
}

/// Whether `name` can only ever address one directory entry:
/// non-empty, neither `.` nor `..`, and free of separators and NUL.
fn is_path_component(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
}

/// The parseable events of a durable transcript, in order.
///
/// Blank lines and lines that are not JSON are skipped — the same
/// tolerance [`synthia::session::JsonlSessionSearch`] applies when it
/// folds a log, so a torn tail left behind by a crash cannot hide the
/// conversation in front of it. An unreadable or empty log yields no
/// events at all, which is what makes both callers read "no
/// transcript" as "unknown session" rather than "empty session".
///
/// The sink's metadata header is **not** an event and is dropped here,
/// not at the call sites: both callers project *events*
/// (`scan_unregistered_transcripts` reads `.next()` as the session's
/// opening row for its `ts` and `subagent_enter`; `get_session` folds
/// the whole list into `history`), so a header left in would report a
/// disk-only session with no `created_at`, no subagent attribution,
/// and a phantom first history entry. A log holding only a header
/// yields no events, which reads as "unknown session" — correct, since
/// a session that never wrote an event has nothing to render.
///
/// Blocking `std::fs` IO: callers on the async runtime route this
/// through `spawn_blocking`.
fn transcript_events(path: &Path) -> Box<dyn Iterator<Item = Value>> {
    let Ok(file) = std::fs::File::open(path) else {
        return Box::new(std::iter::empty());
    };
    Box::new(
        BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
            .filter(|row| !is_metadata_header_row(row)),
    )
}

/// The raw JSONL rows of a durable transcript, in append order, with
/// blank and unparseable lines dropped.
///
/// Unlike [`transcript_events`] this keeps each line's original text
/// **and** the sink's metadata header. An export is an archival
/// artifact whose point is to reproduce the file: re-serialising the
/// parsed value (which may reorder keys) would not be faithful, and
/// dropping the header would strip the on-disk schema version the
/// artifact is supposed to carry. The caller gets the bytes the sink
/// wrote, minus a torn tail.
///
/// Blocking `std::fs` IO: callers on the async runtime route this
/// through `spawn_blocking`.
fn transcript_lines(path: &Path) -> Vec<String> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| {
            !line.trim().is_empty()
                && serde_json::from_str::<Value>(line).is_ok()
        })
        .collect()
}

/// One durable session the disk scan found: id, creation time,
/// and sub-agent attribution when the log opens with the
/// router's `subagent_enter` header.
#[derive(Debug)]
struct ScannedSession {
    id: String,
    created_at: Option<DateTime<Utc>>,
    parent_session_id: Option<String>,
    agent_name: Option<String>,
}

fn scan_unregistered_transcripts(
    sessions_root: &Path,
    user_id: &str,
    registered: &HashSet<String>,
) -> Vec<ScannedSession> {
    let mut found = Vec::new();
    if !is_path_component(user_id) {
        return found;
    }
    let Ok(entries) = std::fs::read_dir(sessions_root.join(user_id)) else {
        // No directory yet (fresh deployment) or unreadable: there is
        // nothing durable to add, which is not an error.
        return found;
    };
    for entry in entries.flatten() {
        let session_id = entry.file_name().to_string_lossy().to_string();
        if registered.contains(&session_id) || is_ephemeral_session(&session_id)
        {
            continue;
        }
        let Some(path) = transcript_path(sessions_root, user_id, &session_id)
        else {
            continue;
        };
        let Some(first) = transcript_events(&path).next() else {
            continue;
        };
        // The first event's timestamp, never `now()`: a legacy log
        // that carries no timestamp reports an unknown creation time.
        let created_at = parse_rfc3339(first.get("ts").and_then(Value::as_str));
        let (parent_session_id, agent_name) =
            subagent_attribution(std::slice::from_ref(&first));
        found.push(ScannedSession {
            id: session_id,
            created_at,
            parent_session_id,
            agent_name,
        });
    }
    found
}

/// GET /api/v1/sessions - List sessions with cursor pagination + filters.
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    AppQuery(query): AppQuery<SessionPageQuery>,
) -> Result<Json<List<SessionSummary>>, AppError> {
    validate_sort(
        query.page.sort.as_deref().unwrap_or("-created_at"),
        SESSION_SORT_WHITELIST,
    )?;
    let resolved = resolve_page(&query.page)?;

    let status_filter = match query.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(parse_status_filter(s)?),
    };
    let context_id_filter = query
        .context_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let user_id = state.default_user_id().to_string();
    let registered = state
        .session_manager
        .list_all()
        .await
        .map_err(|e| Error::session(format!("{e}")))?;
    let registered_ids: HashSet<String> =
        registered.iter().map(|s| s.id.clone()).collect();

    // Everything recorded before this process started is only on
    // disk, so the listing is the union of both sources. The scan
    // skips the ids the registry already covers, which is what keeps
    // the merge free of duplicates.
    let durable = {
        let sessions_root = state.session_manager.sessions_root().to_path_buf();
        let scan_user = user_id.clone();
        tokio::task::spawn_blocking(move || {
            scan_unregistered_transcripts(
                &sessions_root,
                &scan_user,
                &registered_ids,
            )
        })
        .await
        .map_err(|e| Error::session(format!("join transcript scan: {e}")))?
    };

    let mut rows: Vec<SessionSummary> =
        Vec::with_capacity(registered.len() + durable.len());
    for session in registered {
        if is_ephemeral_session(&session.id) {
            continue;
        }
        let sink = state.session_manager.sink(&user_id, &session.id);
        let live =
            live_session_state(&state.active_sessions, &user_id, &session.id);
        // One read serves status, timestamps, and sub-agent
        // attribution — the listing must not open a session's log
        // twice per request.
        let events = sink.read().await.unwrap_or_default();
        let status = infer_status_from_parts(live, !events.is_empty());
        let created_at = events
            .iter()
            .rev()
            .find_map(|ev| parse_rfc3339(ev.get("ts").and_then(Value::as_str)));
        let (parent_session_id, agent_name) = subagent_attribution(&events);
        if !selected(&session.id, status, status_filter, context_id_filter) {
            continue;
        }
        rows.push(SessionSummary {
            id: session.id.clone(),
            status,
            context_id: Some(session.id),
            created_at,
            parent_session_id,
            agent_name,
        });
    }
    // A session known only from disk has no controller, so its status
    // comes from the transcript alone. Its creation time is the first
    // event's timestamp; a session whose transcript is unreadable or
    // empty never reaches this loop, and neither does an ephemeral one
    // — the scan already dropped that namespace before opening its
    // logs.
    for scanned in durable {
        let live =
            live_session_state(&state.active_sessions, &user_id, &scanned.id);
        let status = infer_status_from_parts(live, true);
        if !selected(&scanned.id, status, status_filter, context_id_filter) {
            continue;
        }
        rows.push(SessionSummary {
            id: scanned.id.clone(),
            status,
            context_id: Some(scanned.id),
            created_at: scanned.created_at,
            parent_session_id: scanned.parent_session_id,
            agent_name: scanned.agent_name,
        });
    }

    let field = resolved.sort_field.as_deref().unwrap_or("created_at");
    match field {
        "status" => {
            // The `id` tiebreaker keeps the merged set's order stable
            // across requests: without it a page of equally-labelled
            // sessions (usually every row) has no defined order, and
            // the cursor could repeat or skip entries.
            rows.sort_by(|a, b| {
                a.status.cmp(b.status).then_with(|| a.id.cmp(&b.id))
            });
            if resolved.descending {
                rows.reverse();
            }
        }
        _ => {
            rows.sort_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.id.cmp(&b.id))
            });
            if resolved.descending {
                rows.reverse();
            }
        }
    }

    let list = paginate(rows, &resolved, |r: &SessionSummary| r.id.as_str());
    Ok(Json(list))
}

/// GET /api/v1/sessions/{id} - Get a single session with history.
pub async fn get_session(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Json<SessionDetail>, AppError> {
    validate_resource_name(&id)?;
    let user_id = state.default_user_id().to_string();
    // The registry only knows the sessions this process has opened,
    // so a restart would 404 every earlier conversation. When it has
    // no record we fall back to the durable transcript instead — and
    // only when neither source knows the id is it unknown.
    if state.session_manager.get(&id).await.is_some() {
        let sink = state.session_manager.sink(&user_id, &id);
        let history = sink
            .read()
            .await
            .map_err(|e| Error::session(format!("{e}")))?;
        let live = live_session_state(&state.active_sessions, &user_id, &id);
        let (status, ts) = infer_status(live, &*sink).await;
        let (parent_session_id, agent_name) = subagent_attribution(&history);
        return Ok(Json(SessionDetail {
            id: id.clone(),
            status,
            context_id: id,
            created_at: ts,
            updated_at: ts,
            history,
            artifacts: Vec::new(),
            parent_session_id,
            agent_name,
        }));
    }

    // Read the log straight off disk: `SessionRegistry::sink` would
    // create the directory for whatever id the client sent, turning a
    // lookup into a write. `transcript_path` is what keeps the path
    // inside the sessions root.
    let Some(path) =
        transcript_path(state.session_manager.sessions_root(), &user_id, &id)
    else {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    };
    let history = tokio::task::spawn_blocking(move || {
        transcript_events(&path).collect::<Vec<Value>>()
    })
    .await
    .map_err(|e| Error::session(format!("join transcript read: {e}")))?;
    let Some(created_at) = history
        .first()
        .map(|ev| parse_rfc3339(ev.get("ts").and_then(Value::as_str)))
    else {
        // An empty or unreadable log is as unknown as a missing id:
        // there is no transcript to render.
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    };
    let updated_at = history
        .iter()
        .rev()
        .find_map(|ev| parse_rfc3339(ev.get("ts").and_then(Value::as_str)));
    let (parent_session_id, agent_name) = subagent_attribution(&history);
    Ok(Json(SessionDetail {
        id: id.clone(),
        status: infer_status_from_parts(
            live_session_state(&state.active_sessions, &user_id, &id),
            true,
        ),
        context_id: id,
        // No registry record, so the creation time comes from the
        // transcript's first event — never from `now()`.
        created_at,
        updated_at,
        history,
        artifacts: Vec::new(),
        parent_session_id,
        agent_name,
    }))
}

/// Response envelope for `GET /api/v1/sessions/{id}/events`.
#[derive(Serialize)]
pub struct TypedEventResponse {
    pub id: String,
    pub typed_count: usize,
    pub legacy_count: usize,
    pub typed: Vec<synthia::session::SessionEvent>,
    pub legacy: Vec<Value>,
}
///
/// Returns the session's typed event stream (`SessionEvent`) as a
/// JSON array. Unlike `GET /sessions/{id}` which returns the
/// opaque `history` (mixed legacy + typed), this endpoint
/// exclusively returns events that the typed-event layer can
/// recognise (`synthia::session::SessionEvent::from_value`). Legacy
/// rows are filtered out and reported in a separate `legacy`
/// field so the frontend can still surface them in the UI.
pub async fn list_session_events(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Json<TypedEventResponse>, AppError> {
    validate_resource_name(&id)?;
    if state.session_manager.get(&id).await.is_none() {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    }
    let user_id = state.default_user_id().to_string();
    let sink = state.session_manager.sink(&user_id, &id);
    let raw = sink
        .read()
        .await
        .map_err(|e| Error::session(format!("{e}")))?;
    let mut typed: Vec<synthia::session::SessionEvent> = Vec::new();
    let mut legacy: Vec<Value> = Vec::new();
    for value in raw {
        if let Some(ev) = synthia::session::SessionEvent::from_value(&value) {
            typed.push(ev);
        } else {
            legacy.push(value);
        }
    }
    Ok(Json(TypedEventResponse {
        id,
        typed_count: typed.len(),
        legacy_count: legacy.len(),
        typed,
        legacy,
    }))
}
/// `GET /api/v1/sessions/{id}/status` — live operation status
/// via [`synthia::session::OperationSnapshot`].
///
/// R10-5 / R12 closure. Returns the most recent snapshot the
/// controller published (`Running` / `Completing` / `Cancelled`),
/// or `404` when the session is unknown / has never run. The
/// /status route replaces the free-form `infer_status` string
/// for clients that want the typed state + iteration index +
/// token usage instead of a label.
pub async fn session_status(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Json<synthia::session::OperationSnapshot>, AppError> {
    use synthia::core::Error;
    validate_resource_name(&id)?;
    let user_id = state.default_user_id().to_string();
    let controller = state.active_sessions.get(&(user_id.clone(), id.clone()));
    let controller = match controller {
        Some(c) => c,
        None => {
            return Err(AppError::from(Error::not_found(format!(
                "session '{id}'"
            ))));
        }
    };
    let (_rx, latest) = controller.subscribe_snapshots();
    match latest {
        Some(snap) => Ok(Json(snap)),
        None => Err(AppError::from(Error::not_found(format!(
            "no snapshot yet for session '{id}'"
        )))),
    }
}

/// `DELETE /api/v1/sessions/{id}` — remove a session's durable
/// transcript and forget it in the in-process registry.
///
/// Deletion is **lifecycle policy**, not a sink primitive: the
/// session crate keeps [`SessionSink`](synthia::session::SessionSink)
/// inert (five methods, no `delete`), so *when* a session may vanish
/// and *where* its bytes live are answered here.
///
/// - `204 No Content` — the transcript is gone.
/// - `404 Not Found` — neither the registry nor the durable store
///   knows the id.
/// - `409 Conflict` (`session_busy`) — a turn is running. Cancel it
///   first (`POST /api/v1/chat/sessions/{id}/cancel`): deleting
///   mid-turn would strand the run's sink mid-append and lose the
///   events it is still writing.
///
/// A registered session's controller is closed *before* the files go,
/// so nothing recreates the directory by appending to a deleted path,
/// and the controller is dropped from the active map so a later
/// `get_or_create` does not hand back a closed one. The session
/// search index needs no surgery: it rebuilds per query from the
/// durable tree (see [`session_search`](super::session_search)), so
/// removing the directory is enough for a deleted conversation to
/// stop matching.
pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<StatusCode, AppError> {
    validate_resource_name(&id)?;
    let user_id = state.default_user_id().to_string();
    let key = (user_id.clone(), id.clone());

    // Clone the handle out of the map before awaiting: `close()` is
    // async and a `DashMap` guard must not be held across it.
    let active = state
        .active_sessions
        .get(&key)
        .map(|entry| entry.value().clone());
    if let Some(controller) = active {
        let (_rx, latest) = controller.subscribe_snapshots();
        if latest
            .as_ref()
            .is_some_and(|snap| turn_in_flight(&snap.state))
        {
            return Err(AppError::new(
                StatusCode::CONFLICT,
                Error::internal(format!("session '{id}' is running")),
            )
            .with_code("session_busy")
            .with_message(format!(
                "session '{id}' is running; cancel it before deleting \
                 (POST /api/v1/chat/sessions/{id}/cancel)"
            )));
        }
        controller
            .close()
            .await
            .map_err(|e| Error::session(format!("close session: {e}")))?;
        state.active_sessions.remove(&key);
    }

    let registered = state.session_manager.remove(&id).await.is_some();
    let Some(dir) =
        transcript_path(state.session_manager.sessions_root(), &user_id, &id)
            .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    };
    let removed = tokio::task::spawn_blocking(move || {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    })
    .await
    .map_err(|e| Error::session(format!("join delete: {e}")))?
    .map_err(|e| Error::session(format!("delete session dir: {e}")))?;

    if removed || registered {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::from(Error::not_found(format!("session '{id}'"))))
    }
}

/// True while a turn still owns its log: `Running` (executing) or
/// `Completing` (flushing its final events).
///
/// Deliberately not [`OperationState::is_terminal`]
/// (synthia::session::OperationState::is_terminal), which counts
/// `Completing` as terminal: the state machine has stopped deciding,
/// but the run is still appending during the flush, so its files must
/// not move underneath it.
fn turn_in_flight(state: &synthia::session::OperationState) -> bool {
    matches!(
        state,
        synthia::session::OperationState::Running { .. }
            | synthia::session::OperationState::Completing
    )
}

/// `GET /api/v1/sessions/{id}/export` — the durable transcript as
/// newline-delimited JSON.
///
/// The raw rows in append order, byte-faithful to what the sink
/// wrote, so the artifact can be archived, diffed against another
/// deployment's log, or re-imported without a projection step. This
/// is the *file*; [`get_session`]'s `history` is the projection
/// (typed classification, artifacts lifted out, compaction
/// checkpoints applied).
///
/// `404` when neither the registry nor the durable store knows the
/// id. A registered session with an empty log exports zero rows —
/// "nothing written yet" is not "unknown id".
pub async fn export_session(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Response, AppError> {
    validate_resource_name(&id)?;
    let user_id = state.default_user_id().to_string();
    let Some(path) =
        transcript_path(state.session_manager.sessions_root(), &user_id, &id)
    else {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    };
    let registered = state.session_manager.get(&id).await.is_some();
    let lines = tokio::task::spawn_blocking(move || transcript_lines(&path))
        .await
        .map_err(|e| Error::session(format!("join export: {e}")))?;
    if lines.is_empty() && !registered {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    }
    let mut body = lines.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    // `attachment` so a client downloads the artifact rather than
    // rendering it. `id` passed `validate_resource_name` above (no
    // quote, separator or NUL), so it cannot escape the filename.
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{id}.jsonl\""),
            ),
        ],
        body,
    )
        .into_response())
}

/// Body of `POST /api/v1/sessions/{id}/fork`.
#[derive(Debug, Deserialize, validator::Validate)]
pub struct ForkSessionRequest {
    /// Fork as of this stream index: the child inherits every row up
    /// to and including it. Omitted → the whole durable log.
    #[serde(default)]
    pub from_stream_index: Option<u64>,
}

/// Response of `POST /api/v1/sessions/{id}/fork`.
#[derive(Debug, Serialize)]
pub struct ForkSessionResponse {
    /// The new session — addressable, listable and streamable exactly
    /// like any other conversation.
    pub session_id: String,
    /// The session it was branched from.
    pub forked_from: String,
    /// Durable rows the child inherited.
    pub rows: usize,
}

/// `POST /api/v1/sessions/{id}/fork` — branch a session.
///
/// The child starts as a copy of the parent's durable transcript up to
/// `from_stream_index` (the whole log when omitted) and then diverges:
/// `POST /api/v1/chat/sessions` registers it through the same path, so
/// it is an ordinary session that can be continued, streamed and
/// deleted like any other, while the parent is left untouched. That is
/// the "branch" half of pi's `Navigation` operation; *rewind*
/// (rewriting the parent's own log in place) is a different, more
/// destructive operation and is not part of this route.
///
/// Rows are copied **through the child's sink**, not by writing the
/// child's file behind it: the sink is the trait's only durable write
/// path, and a fork that wrote the file directly would be relying on
/// one backend's layout (the JSONL backend happens to derive its
/// counters from the file it opens; the in-memory one has no file at
/// all).
///
/// The cost of that choice is one durable append per row — each
/// `SessionSink::append` fsyncs — so a fork is O(rows) in fsyncs:
/// measured 63.8 s for a 27 086-row parent. A batch-append method on
/// the sink is the clean way to make it fast, and adding one is a
/// `synthia-session` decision (the trait's five methods are a
/// deliberate ceiling), not something this route should route around.
///
/// The provenance is this response: the child's log is a copy, not a
/// reference, so nothing in it names the parent. A durable
/// `session_fork` row needs the typed event taxonomy to grow one
/// first; until then, callers that care keep the ids.
///
/// `404` when neither the registry nor the durable store knows the
/// parent — forking nothing is not an empty session.
pub async fn fork_session(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
    AppJson(req): AppJson<ForkSessionRequest>,
) -> Result<(StatusCode, Json<ForkSessionResponse>), AppError> {
    validate_resource_name(&id)?;
    let user_id = state.default_user_id().to_string();
    let Some(parent_path) =
        transcript_path(state.session_manager.sessions_root(), &user_id, &id)
    else {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    };
    let registered = state.session_manager.get(&id).await.is_some();
    let lines =
        tokio::task::spawn_blocking(move || transcript_lines(&parent_path))
            .await
            .map_err(|e| Error::session(format!("join fork read: {e}")))?;
    if lines.is_empty() && !registered {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    }
    let inherited = inherited_rows(&lines, req.from_stream_index);

    let child_id = Uuid::new_v4().to_string();
    state
        .get_or_create_session_controller(&user_id, &child_id)
        .await?;
    let child = state.session_manager.sink(&user_id, &child_id);
    // One `fsync` at the tail instead of one per row — a
    // 27 k-row parent used to take 63.8 s of fsync serialisation;
    // the batched path completes in well under a second on the
    // same disk. The fork's durability contract is "atomic":
    // either every inherited row lands or none do, so the
    // controller's resume against the child sees the full
    // transcript immediately.
    child
        .append_many(&inherited)
        .await
        .map_err(|e| Error::session(format!("fork append_many: {e}")))?;

    Ok((
        StatusCode::CREATED,
        Json(ForkSessionResponse {
            session_id: child_id,
            forked_from: id,
            rows: inherited.len(),
        }),
    ))
}

/// The rows a fork inherits: every **event** row whose stream index
/// is at or below `from`, or all of them when `from` is `None`.
///
/// A row's stream index is the stored `stream_index` field when
/// present, else its 1-based ordinal — the same fallback
/// `read_from_index` documents for logs written before the field
/// existed (and for the legacy rows a rebuilt child may still hold).
///
/// The parent's metadata header is **dropped**, not copied. A fork
/// copies a parent's events; the parent's schema header describes the
/// parent's own file (`{"_meta":{"id":"<parent>"}}`) and the child
/// writes its own header on first append. Copying it would land a
/// header-shaped row *mid-log* in the child, where
/// `JsonlSessionSink::read_filtered` — which strips only the first
/// line's header — would parse it as a phantom event and inflate
/// `last_event_seq` by one. Dropping it up front also keeps the
/// ordinal fallback aligned: the sink's seq numbering skips the
/// header, so leaving it in would shift every fallback ordinal by one.
fn inherited_rows(lines: &[String], from: Option<u64>) -> Vec<Value> {
    let parsed: Vec<Value> = lines
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| !is_metadata_header_row(row))
        .collect();
    let Some(from) = from else {
        return parsed;
    };
    parsed
        .into_iter()
        .enumerate()
        .filter(|(ordinal, row)| {
            let index = row
                .get("stream_index")
                .and_then(Value::as_u64)
                .unwrap_or(*ordinal as u64 + 1);
            index <= from
        })
        .map(|(_, row)| row)
        .collect()
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use synthia::session::{SessionSink as _, manager::SERVER_DEFAULT_USER_ID};

    use super::*;

    #[test]
    fn parse_status_filter_accepts_all_known_labels() {
        for (label, expected) in [
            ("unspecified", "unspecified"),
            ("submitted", "submitted"),
            ("working", "working"),
            ("completed", "completed"),
            ("failed", "failed"),
            ("canceled", "canceled"),
            ("input_required", "input_required"),
            ("rejected", "rejected"),
            ("auth_required", "auth_required"),
        ] {
            assert_eq!(parse_status_filter(label).unwrap(), expected);
        }
    }

    #[test]
    fn parse_status_filter_rejects_unknown_label() {
        let err =
            parse_status_filter("not_a_state").expect_err("unknown label");
        assert!(matches!(err, synthia::core::Error::InvalidItem { .. }));
        assert!(err.to_string().contains("status filter 'not_a_state'"));
    }

    #[test]

    fn parse_status_filter_is_case_sensitive() {
        let err = parse_status_filter("Completed").expect_err("uppercase");
        assert!(matches!(err, synthia::core::Error::InvalidItem { .. }));
    }

    #[test]
    fn parse_rfc3339_returns_none_for_invalid_input() {
        assert!(parse_rfc3339(None).is_none());
        assert!(parse_rfc3339(Some("not-a-date")).is_none());
    }

    /// The namespace test is "the id starts with the prefix", and the
    /// prefix carries its own `-` separator. `anthropic-<uuid>` is the
    /// mint; an id that merely shares the letters is not, and neither
    /// is the empty string or a bare prefixless name.
    #[test]
    fn is_ephemeral_session_matches_only_the_separated_namespace() {
        for id in ["anthropic-1f9c", "anthropic-", "anthropic-0"] {
            assert!(is_ephemeral_session(id), "{id} is in the namespace");
        }
        for id in ["anthropicx-1", "anthropic", "xanthropic-1", "chat-1", ""] {
            assert!(!is_ephemeral_session(id), "{id} is not in the namespace");
        }
    }

    #[test]
    fn parse_rfc3339_roundtrips_valid_timestamp() {
        let raw = "2026-08-22T10:00:00+00:00";
        let parsed = parse_rfc3339(Some(raw)).expect("valid");
        assert_eq!(parsed.to_rfc3339(), "2026-08-22T10:00:00+00:00");
    }

    /// Non-empty sink with no live controller ⇒ the run finished
    /// ⇒ "completed". This is the regression guard for the bug
    /// where every finished session was reported "working"
    /// forever because `SessionEnded` is ephemeral and never
    /// reaches the JSONL sink.
    #[tokio::test]
    async fn infer_status_reports_completed_without_live_controller() {
        let sink = synthia::session::in_memory::InMemorySessionSink::new("s1");
        sink.append(&serde_json::json!({
            "type": "Model",
            "data": {"text": "hi"},
            "ts": "2026-08-22T10:00:00+00:00",
        }))
        .await
        .expect("append");
        let (status, ts) = infer_status(None, &sink).await;
        assert_eq!(status, "completed");
        assert!(ts.is_some(), "ts must come from the last event");
    }

    /// A live Running controller ⇒ "working" even before any
    /// durable event has been persisted.
    #[tokio::test]
    async fn infer_status_reports_working_for_running_controller() {
        let sink = synthia::session::in_memory::InMemorySessionSink::new("s1");
        let (status, _) =
            infer_status(Some(SessionState::Running), &sink).await;
        assert_eq!(status, "working");
    }

    /// Empty sink + no live run ⇒ "unspecified" (never touched).
    #[tokio::test]
    async fn infer_status_reports_unspecified_for_untouched_session() {
        let sink = synthia::session::in_memory::InMemorySessionSink::new("s1");
        let (status, ts) = infer_status(None, &sink).await;
        assert_eq!(status, "unspecified");
        assert!(ts.is_none());
    }

    /// A cancelled controller with durable history ⇒ "canceled".
    #[tokio::test]
    async fn infer_status_reports_canceled_for_cancelled_controller() {
        let sink = synthia::session::in_memory::InMemorySessionSink::new("s1");
        sink.append(&serde_json::json!({
            "type": "UserInput",
            "data": {"text": "go"},
        }))
        .await
        .expect("append");
        let (status, _) =
            infer_status(Some(SessionState::Cancelled), &sink).await;
        assert_eq!(status, "canceled");
    }

    /// `live_session_state` maps the DashMap key to the
    /// controller state and returns `None` when absent.
    #[tokio::test]
    async fn live_session_state_none_when_controller_absent() {
        let map: DashMap<(String, String), Arc<SessionController>> =
            DashMap::new();
        assert_eq!(live_session_state(&map, "alice", "s-missing"), None);
    }

    #[test]
    fn session_detail_serialises_with_legacy_artifacts_field() {
        let detail = SessionDetail {
            id: "abc".into(),
            status: "completed",
            context_id: "abc".into(),
            created_at: None,
            updated_at: None,
            history: vec![
                serde_json::json!({"type": "Model", "data": {"text": "hi"}}),
            ],
            artifacts: Vec::new(),
            parent_session_id: None,
            agent_name: None,
        };
        let v = serde_json::to_value(&detail).expect("serialise");
        assert_eq!(v["id"], "abc");
        assert_eq!(v["status"], "completed");
        assert!(v["history"].is_array());
        // The frontend relies on the `artifacts` field being
        // present (even as `[]`) so it can iterate without
        // special-casing undefined.
        assert!(v["artifacts"].is_array());
    }

    /// R5-10: `TypedEventResponse` serialises the typed event
    /// split (typed / legacy) with counts. Pin the field shape
    /// so the JSON contract is locked.
    #[test]
    fn typed_event_response_serialises_with_counts() {
        let resp = TypedEventResponse {
            id: "abc".to_string(),
            typed_count: 2,
            legacy_count: 1,
            typed: Vec::new(),
            legacy: Vec::new(),
        };
        let v = serde_json::to_value(&resp).expect("serialise");
        assert_eq!(v["id"], "abc");
        assert_eq!(v["typed_count"], 2);
        assert_eq!(v["legacy_count"], 1);
        assert_eq!(v["typed"], serde_json::json!([]));
        assert_eq!(v["legacy"], serde_json::json!([]));
    }

    /// A hermetic `AppState` over a temp workspace: these tests build a
    /// real `sessions/<user>/<id>/events.jsonl` tree, so the caller
    /// keeps the tempdir alive.
    async fn durable_state() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp workspace");
        let sessions = synthia::session::manager::SessionRegistry::new(
            dir.path().join("sessions"),
        );
        let state =
            AppState::for_test(sessions, dir.path().to_path_buf()).await;
        (Arc::new(state), dir)
    }

    /// Write `lines` to the transcript of `session_id`, the layout a
    /// previous process leaves behind.
    fn write_transcript(
        workspace: &std::path::Path,
        session_id: &str,
        lines: &[Value],
    ) {
        let dir = workspace
            .join("sessions")
            .join(SERVER_DEFAULT_USER_ID)
            .join(session_id);
        std::fs::create_dir_all(&dir).expect("session dir");
        let body: String =
            lines.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(dir.join("events.jsonl"), body).expect("transcript");
    }

    /// The session directory for `session_id` under a temp workspace.
    fn session_dir(
        workspace: &std::path::Path,
        session_id: &str,
    ) -> std::path::PathBuf {
        workspace
            .join("sessions")
            .join(SERVER_DEFAULT_USER_ID)
            .join(session_id)
    }

    /// `DELETE` removes the log *and* the directory, and the id stops
    /// being addressable through either read surface.
    #[tokio::test]
    async fn delete_session_removes_the_transcript_and_the_registry_entry() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "doomed",
            &[serde_json::json!({
                "type": "UserInput",
                "ts": "2026-09-25T08:00:00+00:00",
                "data": {"text": "hello"},
            })],
        );

        let status = delete_session(
            State(Arc::clone(&state)),
            AppPath("doomed".to_string()),
        )
        .await
        .expect("delete");
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(
            !session_dir(dir.path(), "doomed").exists(),
            "the whole session directory must go, not just its log",
        );

        match get_session(
            State(Arc::clone(&state)),
            AppPath("doomed".to_string()),
        )
        .await
        {
            Err(err) => assert_eq!(err.status_code, StatusCode::NOT_FOUND),
            Ok(_) => panic!("a deleted session must not be readable"),
        }
        let Json(list) = list_sessions(
            State(Arc::clone(&state)),
            AppQuery(SessionPageQuery::new()),
        )
        .await
        .expect("list");
        assert!(
            !list.data.iter().any(|row| row.id == "doomed"),
            "a deleted session must not stay in the listing",
        );
    }

    /// An id neither source knows is a 404 — and the delete creates
    /// nothing on the way there.
    #[tokio::test]
    async fn delete_session_404s_for_an_unknown_id() {
        let (state, dir) = durable_state().await;
        match delete_session(
            State(Arc::clone(&state)),
            AppPath("never-existed".to_string()),
        )
        .await
        {
            Err(err) => assert_eq!(err.status_code, StatusCode::NOT_FOUND),
            Ok(status) => panic!("an unknown id must not 204: {status}"),
        }
        assert!(
            !session_dir(dir.path(), "never-existed").exists(),
            "a 404 must not leave an empty session directory behind",
        );
    }

    /// Export is the file, not the projection: append order kept, key
    /// order in each row preserved, blank and torn lines dropped.
    #[tokio::test]
    async fn export_session_returns_the_raw_durable_rows() {
        use http_body_util::BodyExt as _;

        let (state, dir) = durable_state().await;
        let session = session_dir(dir.path(), "exportable");
        std::fs::create_dir_all(&session).expect("session dir");
        // `b` before `a` is deliberate: a parse-then-reserialise
        // projection would reorder it, and a torn final line must not
        // reach the archive.
        std::fs::write(
            session.join("events.jsonl"),
            "{\"b\":1,\"a\":2}\n\n{\"type\":\"Model\"}\n{\"torn\":\n",
        )
        .expect("transcript");

        let resp = export_session(
            State(Arc::clone(&state)),
            AppPath("exportable".to_string()),
        )
        .await
        .expect("export");
        assert_eq!(resp.status(), StatusCode::OK);
        let header_value = |name: header::HeaderName| {
            resp.headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        assert_eq!(
            header_value(header::CONTENT_TYPE).as_deref(),
            Some("application/x-ndjson"),
        );
        assert_eq!(
            header_value(header::CONTENT_DISPOSITION).as_deref(),
            Some("attachment; filename=\"exportable.jsonl\""),
        );
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("export body")
            .to_bytes();
        assert_eq!(
            std::str::from_utf8(&bytes).expect("utf-8"),
            "{\"b\":1,\"a\":2}\n{\"type\":\"Model\"}\n",
        );
    }

    /// A registered session whose log is empty exports zero rows: it
    /// is not an unknown id.
    #[tokio::test]
    async fn export_session_serves_a_registered_session_with_an_empty_log() {
        use http_body_util::BodyExt as _;

        let (state, _dir) = durable_state().await;
        state
            .session_manager
            .create_with_user(
                "blank".to_string(),
                SERVER_DEFAULT_USER_ID.to_string(),
            )
            .await
            .expect("register");

        let resp = export_session(
            State(Arc::clone(&state)),
            AppPath("blank".to_string()),
        )
        .await
        .expect("export");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("export body")
            .to_bytes();
        assert!(bytes.is_empty(), "an empty log exports nothing: {bytes:?}");
    }

    /// Export of an id neither source knows is a 404.
    #[tokio::test]
    async fn export_session_404s_for_an_unknown_id() {
        let (state, _dir) = durable_state().await;
        match export_session(
            State(Arc::clone(&state)),
            AppPath("never-existed".to_string()),
        )
        .await
        {
            Err(err) => assert_eq!(err.status_code, StatusCode::NOT_FOUND),
            Ok(_) => panic!("an unknown id must not export"),
        }
    }

    /// Fork copies the parent's transcript into a new session that is
    /// addressable on both read surfaces, and leaves the parent alone.
    #[tokio::test]
    async fn fork_session_copies_the_transcript_into_an_addressable_child() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "origin",
            &[
                serde_json::json!({
                    "type": "UserInput",
                    "ts": "2026-09-25T08:00:00+00:00",
                    "data": {"text": "hello"},
                }),
                serde_json::json!({
                    "type": "Model",
                    "ts": "2026-09-25T08:00:05+00:00",
                    "data": {"text": "hi"},
                }),
            ],
        );

        let (status, Json(forked)) = fork_session(
            State(Arc::clone(&state)),
            AppPath("origin".to_string()),
            AppJson(ForkSessionRequest {
                from_stream_index: None,
            }),
        )
        .await
        .expect("fork");
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(forked.rows, 2);
        assert_eq!(forked.forked_from, "origin");
        assert_ne!(forked.session_id, "origin");

        let Json(child) = get_session(
            State(Arc::clone(&state)),
            AppPath(forked.session_id.clone()),
        )
        .await
        .expect("the child is a session");
        assert_eq!(child.history.len(), 2, "the child inherits the prefix");

        let Json(parent) = get_session(
            State(Arc::clone(&state)),
            AppPath("origin".to_string()),
        )
        .await
        .expect("the parent still reads");
        assert_eq!(parent.history.len(), 2, "the parent is untouched");
    }

    /// A parent written **through the sink** carries a schema-version
    /// metadata header on its first line. The fork must copy the
    /// parent's events, not that header: a copied header would land
    /// mid-log in the child, where `read_filtered` (which strips only
    /// the first line) parses it as a phantom event and inflates
    /// `last_event_seq`. The headerless `write_transcript` fixture
    /// above cannot exercise this path, so this test seeds the parent
    /// via `SessionSink::append`.
    #[tokio::test]
    async fn fork_session_drops_the_parent_metadata_header() {
        let (state, _dir) = durable_state().await;
        let parent = state
            .session_manager
            .sink(SERVER_DEFAULT_USER_ID, "meta-origin");
        parent
            .append(&serde_json::json!({
                "type": "UserInput",
                "data": {"text": "hello"},
            }))
            .await
            .expect("seed parent row 1");
        parent
            .append(&serde_json::json!({
                "type": "Model",
                "data": {"text": "hi"},
            }))
            .await
            .expect("seed parent row 2");
        let parent_seq = parent.snapshot().await.expect("parent snapshot");
        assert_eq!(
            parent_seq.last_event_seq, 2,
            "the parent's header must not count as an event",
        );

        let (status, Json(forked)) = fork_session(
            State(Arc::clone(&state)),
            AppPath("meta-origin".to_string()),
            AppJson(ForkSessionRequest {
                from_stream_index: None,
            }),
        )
        .await
        .expect("fork");
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(
            forked.rows, 2,
            "the child inherits the parent's 2 events, not its header",
        );

        let child = state
            .session_manager
            .sink(SERVER_DEFAULT_USER_ID, &forked.session_id);
        let child_rows = child.read().await.expect("child read");
        assert_eq!(
            child_rows.len(),
            2,
            "no phantom `_meta` row in the child: {child_rows:?}",
        );
        assert!(
            child_rows.iter().all(|row| row.get("_meta").is_none()),
            "the child must hold events only: {child_rows:?}",
        );
        assert_eq!(
            child
                .snapshot()
                .await
                .expect("child snapshot")
                .last_event_seq,
            2,
            "the child's seq counts its own header + the 2 events only",
        );
    }

    /// `from_stream_index` forks as of a point: the rows past it stay
    /// behind.
    #[tokio::test]
    async fn fork_session_truncates_at_the_requested_stream_index() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "long",
            &[
                serde_json::json!({
                    "stream_index": 1,
                    "type": "UserInput",
                    "data": {"text": "a"},
                }),
                serde_json::json!({
                    "stream_index": 2,
                    "type": "Model",
                    "data": {"text": "b"},
                }),
                serde_json::json!({
                    "stream_index": 3,
                    "type": "UserInput",
                    "data": {"text": "c"},
                }),
            ],
        );

        let (status, Json(forked)) = fork_session(
            State(Arc::clone(&state)),
            AppPath("long".to_string()),
            AppJson(ForkSessionRequest {
                from_stream_index: Some(2),
            }),
        )
        .await
        .expect("fork");
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(forked.rows, 2, "only the rows at or below index 2");

        let Json(child) = get_session(
            State(Arc::clone(&state)),
            AppPath(forked.session_id.clone()),
        )
        .await
        .expect("the child is a session");
        assert_eq!(child.history.len(), 2);
        assert_eq!(child.history[1]["data"]["text"], "b");
    }

    /// The child's future is its own: an append after the fork lands in
    /// the child's log only, at the ordinal *after* the inherited rows —
    /// the sink owns that counter, which is why the copy goes through it.
    #[tokio::test]
    async fn fork_session_gives_the_child_an_independent_future() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "base",
            &[
                serde_json::json!({
                    "type": "UserInput",
                    "ts": "2026-09-25T08:00:00+00:00",
                    "data": {"text": "alpha"},
                }),
                serde_json::json!({
                    "type": "Model",
                    "ts": "2026-09-25T08:00:05+00:00",
                    "data": {"text": "beta"},
                }),
            ],
        );

        let (_status, Json(forked)) = fork_session(
            State(Arc::clone(&state)),
            AppPath("base".to_string()),
            AppJson(ForkSessionRequest {
                from_stream_index: None,
            }),
        )
        .await
        .expect("fork");

        let child_sink = state
            .session_manager
            .sink(SERVER_DEFAULT_USER_ID, &forked.session_id);
        let seq = child_sink
            .append(&serde_json::json!({
                "type": "UserInput",
                "ts": "2026-09-25T09:00:00+00:00",
                "data": {"text": "gamma"},
            }))
            .await
            .expect("append to the child");
        assert_eq!(seq, 3, "the inherited rows occupy 1..=2");

        let Json(child) = get_session(
            State(Arc::clone(&state)),
            AppPath(forked.session_id.clone()),
        )
        .await
        .expect("the child is a session");
        assert_eq!(child.history.len(), 3);
        let Json(parent) =
            get_session(State(Arc::clone(&state)), AppPath("base".to_string()))
                .await
                .expect("the parent still reads");
        assert_eq!(
            parent.history.len(),
            2,
            "the parent must not gain the child's row",
        );
    }

    /// A registered session with no rows forks into an empty child: it
    /// is not an unknown id.
    #[tokio::test]
    async fn fork_session_of_a_registered_session_with_an_empty_log() {
        let (state, _dir) = durable_state().await;
        state
            .session_manager
            .create_with_user(
                "fresh".to_string(),
                SERVER_DEFAULT_USER_ID.to_string(),
            )
            .await
            .expect("register");

        let (status, Json(forked)) = fork_session(
            State(Arc::clone(&state)),
            AppPath("fresh".to_string()),
            AppJson(ForkSessionRequest {
                from_stream_index: None,
            }),
        )
        .await
        .expect("fork");
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(forked.rows, 0);

        let Json(child) = get_session(
            State(Arc::clone(&state)),
            AppPath(forked.session_id.clone()),
        )
        .await
        .expect("the child is registered");
        assert!(child.history.is_empty());
    }

    /// Forking an id neither source knows is a 404, and creates nothing.
    #[tokio::test]
    async fn fork_session_404s_for_an_unknown_id() {
        let (state, dir) = durable_state().await;
        match fork_session(
            State(Arc::clone(&state)),
            AppPath("never-existed".to_string()),
            AppJson(ForkSessionRequest {
                from_stream_index: None,
            }),
        )
        .await
        {
            Err(err) => assert_eq!(err.status_code, StatusCode::NOT_FOUND),
            Ok(_) => panic!("an unknown parent must not fork"),
        }
        assert!(
            !session_dir(dir.path(), "never-existed").exists(),
            "a 404 must not leave an empty session directory behind",
        );
    }

    /// A turn that is still writing owns its log. `Completing` counts
    /// as busy as well, even though `is_terminal` reports it terminal:
    /// the flush is still appending.
    #[test]
    fn turn_in_flight_covers_running_and_completing_only() {
        use synthia::session::OperationState;

        assert!(turn_in_flight(&OperationState::Running { iteration: 0 }));
        assert!(turn_in_flight(&OperationState::Completing));
        assert!(!turn_in_flight(&OperationState::Idle));
        assert!(!turn_in_flight(&OperationState::Failed {
            error_message: "boom".to_string(),
        }));
        assert!(!turn_in_flight(&OperationState::Cancelled {
            reason: "stop".to_string(),
        }));
    }

    /// The listing reaches a conversation recorded by an earlier
    /// process: the registry is empty after a restart, so before the
    /// merge every pre-restart conversation vanished from the Sessions
    /// page.
    #[tokio::test]
    async fn list_sessions_includes_a_session_known_only_from_disk() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "earlier-run",
            &[
                serde_json::json!({
                    "type": "UserInput",
                    "ts": "2026-09-19T08:00:00+00:00",
                    "data": {"text": "hello"},
                }),
                serde_json::json!({
                    "type": "Model",
                    "ts": "2026-09-19T08:00:05+00:00",
                    "data": {"text": "hi"},
                }),
            ],
        );

        let Json(list) = list_sessions(
            State(Arc::clone(&state)),
            AppQuery(SessionPageQuery::new()),
        )
        .await
        .expect("list");

        let row = list
            .data
            .iter()
            .find(|row| row.id == "earlier-run")
            .expect("a durable transcript must be listed");
        assert_eq!(row.status, "completed");
        assert_eq!(row.context_id.as_deref(), Some("earlier-run"));
        // The creation time is the transcript's first event, not the
        // time of the request.
        assert_eq!(
            row.created_at.expect("first event ts").to_rfc3339(),
            "2026-09-19T08:00:00+00:00"
        );
    }

    /// A disk-only transcript written **through the sink** opens with
    /// the sink's schema-version metadata header. Both consumers of
    /// `transcript_events` must project events only: the listing reads
    /// the session's `created_at` off its first row, and `get_session`
    /// folds the rows into `history`. A header left in would report no
    /// creation time, no subagent attribution, and a phantom first
    /// history entry. `write_transcript` writes headerless fixtures,
    /// so the neighbouring test cannot catch this.
    #[tokio::test]
    async fn disk_only_projections_skip_the_metadata_header() {
        let (state, _dir) = durable_state().await;
        // A sink writes the header; it does NOT register the session
        // in `SessionRegistry::sessions`, so this stays disk-only —
        // the exact shape the restart path produces.
        let sink = state
            .session_manager
            .sink(SERVER_DEFAULT_USER_ID, "pre-restart");
        sink.append(&serde_json::json!({
            "type": "UserInput",
            "ts": "2026-09-19T08:00:00+00:00",
            "data": {"text": "hello"},
        }))
        .await
        .expect("seed row 1");
        sink.append(&serde_json::json!({
            "type": "Model",
            "ts": "2026-09-19T08:00:05+00:00",
            "data": {"text": "hi"},
        }))
        .await
        .expect("seed row 2");

        let Json(list) = list_sessions(
            State(Arc::clone(&state)),
            AppQuery(SessionPageQuery::new()),
        )
        .await
        .expect("list");
        let row = list
            .data
            .iter()
            .find(|row| row.id == "pre-restart")
            .expect("a durable transcript must be listed");
        assert_eq!(
            row.created_at.expect("first *event* ts").to_rfc3339(),
            "2026-09-19T08:00:00+00:00",
            "the header row has no `ts`; it must not shadow the first event",
        );

        let Json(detail) = get_session(
            State(Arc::clone(&state)),
            AppPath("pre-restart".to_string()),
        )
        .await
        .expect("the disk-only session is readable");
        assert_eq!(
            detail.history.len(),
            2,
            "history must hold the 2 events, not the header row",
        );
        assert!(
            detail.history.iter().all(|row| row.get("_meta").is_none()),
            "no `_meta` row may reach the history projection",
        );
        assert_eq!(
            detail.created_at.expect("first *event* ts").to_rfc3339(),
            "2026-09-19T08:00:00+00:00",
        );
    }

    /// A session the process registered *and* has a transcript for is
    /// one row: the listing merges the two sources, it does not stack
    /// them.
    #[tokio::test]
    async fn list_sessions_lists_a_registered_session_once() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "both-sources",
            &[serde_json::json!({
                "type": "UserInput",
                "ts": "2026-09-19T08:00:00+00:00",
                "data": {"text": "hello"},
            })],
        );
        state
            .session_manager
            .create_with_user(
                "both-sources".to_string(),
                SERVER_DEFAULT_USER_ID.to_string(),
            )
            .await
            .expect("register");

        let Json(list) = list_sessions(
            State(Arc::clone(&state)),
            AppQuery(SessionPageQuery::new()),
        )
        .await
        .expect("list");

        assert_eq!(
            list.data
                .iter()
                .filter(|row| row.id == "both-sources")
                .count(),
            1,
            "a session in both sources must be listed once"
        );
        assert_eq!(list.total, Some(1));
    }

    /// A legacy log whose events carry no timestamp is still a session:
    /// it is listed, with an unknown creation time rather than a
    /// manufactured one.
    #[tokio::test]
    async fn list_sessions_reports_no_created_at_for_an_untimed_log() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "legacy-log",
            &[serde_json::json!({
                "type": "UserInput",
                "data": {"text": "hello"},
            })],
        );

        let Json(list) = list_sessions(
            State(Arc::clone(&state)),
            AppQuery(SessionPageQuery::new()),
        )
        .await
        .expect("list");

        let row = list
            .data
            .iter()
            .find(|row| row.id == "legacy-log")
            .expect("an untimed log is still a session");
        assert_eq!(row.created_at, None);
    }

    /// The detail view renders a transcript the registry never saw —
    /// the "View Detail" 404 reported for pre-restart sessions.
    #[tokio::test]
    async fn get_session_reads_a_transcript_the_registry_never_saw() {
        let (state, dir) = durable_state().await;
        write_transcript(
            dir.path(),
            "earlier-run",
            &[
                serde_json::json!({
                    "type": "UserInput",
                    "ts": "2026-09-19T08:00:00+00:00",
                    "data": {"text": "hello"},
                }),
                serde_json::json!({
                    "type": "Model",
                    "ts": "2026-09-19T08:00:05+00:00",
                    "data": {"text": "hi"},
                }),
            ],
        );

        let Json(detail) = get_session(
            State(Arc::clone(&state)),
            AppPath("earlier-run".to_string()),
        )
        .await
        .expect("durable detail");

        assert_eq!(detail.id, "earlier-run");
        assert_eq!(detail.context_id, "earlier-run");
        assert_eq!(detail.status, "completed");
        assert_eq!(detail.history.len(), 2);
        assert_eq!(detail.history[0]["data"]["text"], "hello");
        assert_eq!(
            detail.created_at.expect("first event ts").to_rfc3339(),
            "2026-09-19T08:00:00+00:00"
        );
        assert_eq!(
            detail.updated_at.expect("last event ts").to_rfc3339(),
            "2026-09-19T08:00:05+00:00"
        );
        assert!(detail.artifacts.is_empty());
    }

    /// Neither source knows the id. A directory whose log is empty is
    /// not a session with no history — there is no transcript to
    /// render, so it is unknown, and it is never listed as a
    /// placeholder row.
    #[tokio::test]
    async fn get_session_404s_without_a_transcript() {
        let (state, dir) = durable_state().await;
        let unused = dir
            .path()
            .join("sessions")
            .join(SERVER_DEFAULT_USER_ID)
            .join("touched-but-never-used");
        std::fs::create_dir_all(&unused).expect("session dir");
        std::fs::write(unused.join("events.jsonl"), "").expect("empty log");

        for id in ["does-not-exist", "touched-but-never-used"] {
            let Err(err) =
                get_session(State(Arc::clone(&state)), AppPath(id.to_string()))
                    .await
            else {
                panic!("{id} must 404");
            };
            assert_eq!(err.status_code, StatusCode::NOT_FOUND, "{id}");
            assert_eq!(err.code, "not_found", "{id}");
        }

        let Json(list) = list_sessions(
            State(Arc::clone(&state)),
            AppQuery(SessionPageQuery::new()),
        )
        .await
        .expect("list");
        assert!(
            list.data.is_empty(),
            "an empty transcript must not be listed"
        );
    }

    /// A traversal id can never address a file outside the sessions
    /// root, even one that exists: the id is rejected, not resolved.
    #[tokio::test]
    async fn get_session_rejects_a_traversal_id() {
        let (state, dir) = durable_state().await;
        // `<root>/sessions/dev/../escape/events.jsonl` — where
        // `../escape` lands if the id is joined without a check.
        let escape = dir.path().join("sessions").join("escape");
        std::fs::create_dir_all(&escape).expect("escape dir");
        std::fs::write(
            escape.join("events.jsonl"),
            "{\"type\":\"UserInput\",\"ts\":\"2026-09-19T08:00:00+00:00\"}\n",
        )
        .expect("escape transcript");

        // `..%2f..%2fetc%2fpasswd` reaches the handler undecoded as a
        // raw path parameter, so both spellings must be refused.
        for id in ["../escape", "../../etc/passwd", "..%2f..%2fetc%2fpasswd"] {
            let Err(err) =
                get_session(State(Arc::clone(&state)), AppPath(id.to_string()))
                    .await
            else {
                panic!("{id} must be refused");
            };
            assert_eq!(err.status_code, StatusCode::BAD_REQUEST, "{id}");
            assert_eq!(err.code, "invalid_item", "{id}");
        }
    }
}
