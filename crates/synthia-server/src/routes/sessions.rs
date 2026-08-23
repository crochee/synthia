//! Session management handlers.
//!
//! [`list_sessions`] enumerates sessions recorded by the in-memory
//! [`synthia_session::manager::SessionRegistry`]. The endpoint shape
//! is intentionally aligned with the chat surface (`/api/v1/chat/
//! sessions`) so callers can reuse the same cursor pagination and
//! filter semantics; only the URL path is different (`/api/v1/
//! sessions` is the read-optimised management listing used by the
//! `SessionsPage`, `/api/v1/chat/sessions` is the chat surface used
//! by `ChatPage`).
//!
//! [`get_session`] fetches a single session's events directly from
//! the session sink so the frontend's `SessionDetailPage` can render
//! the full JSONL transcript.

// Allow `result_large_err`: `parse_status_filter` returns the
// un-boxed `synthia_core::Error` (≥128 bytes once the RFC-0977
// common fields are counted) — same accepted trade-off as
// `api/v1/cursor.rs` and `api/v1/validation.rs`; boxing would
// force `.map_err(|e| *e)` at every call site.
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Serialize;
use serde_json::Value;
use synthia_core::Error;

use super::helpers::paginate;
use crate::{
    api::{
        AppError,
        AppPath,
        AppQuery,
        List,
        MAX_LIMIT,
        SessionPageQuery,
        resolve_page,
        validate_resource_name,
        validate_sort,
    },
    session::controller::{SessionController, SessionState},
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
async fn infer_status(
    live_state: Option<SessionState>,
    sink: &dyn synthia_session::SessionSink,
) -> (&'static str, Option<DateTime<Utc>>) {
    let events = match sink.read().await {
        Ok(events) => events,
        Err(_) => return ("unspecified", None),
    };
    let ts = events
        .iter()
        .rev()
        .find_map(|ev| parse_rfc3339(ev.get("ts").and_then(|t| t.as_str())));

    if matches!(live_state, Some(SessionState::Running)) {
        return ("working", ts);
    }
    if events.is_empty() {
        return ("unspecified", None);
    }
    match live_state {
        Some(SessionState::Cancelled) => ("canceled", ts),
        _ => ("completed", ts),
    }
}

fn parse_rfc3339(raw: Option<&str>) -> Option<DateTime<Utc>> {
    raw.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    })
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
    let sessions = state
        .session_manager
        .list_all()
        .await
        .map_err(|e| Error::session(format!("{e}")))?;

    let mut rows: Vec<SessionSummary> = Vec::with_capacity(sessions.len());
    for session in sessions {
        let sink = state.session_manager.sink(&user_id, &session.id);
        let live =
            live_session_state(&state.active_sessions, &user_id, &session.id);
        let (status, created_at) = infer_status(live, &*sink).await;
        if let Some(filter) = status_filter
            && filter != status
        {
            continue;
        }
        // `context_id` is currently identical to `id` (sessions have no
        // separate context-id column), so the filter would be tautological.
        // We honor it as a strict equality match against the session id,
        // which is what a client would expect when narrowing the list.
        if let Some(needle) = context_id_filter
            && session.id != needle
        {
            continue;
        }
        rows.push(SessionSummary {
            id: session.id.clone(),
            status,
            context_id: Some(session.id.clone()),
            created_at,
        });
    }

    let field = resolved.sort_field.as_deref().unwrap_or("created_at");
    match field {
        "status" => {
            rows.sort_by(|a, b| a.status.cmp(b.status));
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

    let limit = query.page.limit.unwrap_or(MAX_LIMIT);
    let _ = limit; // limit is encoded in `resolved.effective_limit`.
    let list = paginate(rows, &resolved, |r: &SessionSummary| r.id.as_str());
    Ok(Json(list))
}

/// GET /api/v1/sessions/{id} - Get a single session with history.
pub async fn get_session(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Json<SessionDetail>, AppError> {
    validate_resource_name(&id)?;
    // The session sink directory is created on first access
    // by `sink()`, so we cannot rely on a missing directory to
    // distinguish "unknown session" from "known but never
    // touched". Instead consult the in-memory registry first
    // and map any unknown id to 404 so clients see a clean
    // signal instead of an opaque 500.
    if state.session_manager.get(&id).await.is_none() {
        return Err(AppError::from(Error::not_found(format!(
            "session '{id}'"
        ))));
    }
    let user_id = state.default_user_id().to_string();
    let sink = state.session_manager.sink(&user_id, &id);
    let history = sink
        .read()
        .await
        .map_err(|e| Error::session(format!("{e}")))?;
    let live = live_session_state(&state.active_sessions, &user_id, &id);
    let (status, ts) = infer_status(live, &*sink).await;
    Ok(Json(SessionDetail {
        id: id.clone(),
        status,
        context_id: id,
        created_at: ts,
        updated_at: ts,
        history,
        artifacts: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use synthia_session::SessionSink as _;

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
        assert!(matches!(err, synthia_core::Error::InvalidItem { .. }));
        assert!(err.to_string().contains("status filter 'not_a_state'"));
    }

    #[test]

    fn parse_status_filter_is_case_sensitive() {
        let err = parse_status_filter("Completed").expect_err("uppercase");
        assert!(matches!(err, synthia_core::Error::InvalidItem { .. }));
    }

    #[test]
    fn parse_rfc3339_returns_none_for_invalid_input() {
        assert!(parse_rfc3339(None).is_none());
        assert!(parse_rfc3339(Some("not-a-date")).is_none());
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
        let sink = synthia_session::in_memory::InMemorySessionSink::new("s1");
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
        let sink = synthia_session::in_memory::InMemorySessionSink::new("s1");
        let (status, _) =
            infer_status(Some(SessionState::Running), &sink).await;
        assert_eq!(status, "working");
    }

    /// Empty sink + no live run ⇒ "unspecified" (never touched).
    #[tokio::test]
    async fn infer_status_reports_unspecified_for_untouched_session() {
        let sink = synthia_session::in_memory::InMemorySessionSink::new("s1");
        let (status, ts) = infer_status(None, &sink).await;
        assert_eq!(status, "unspecified");
        assert!(ts.is_none());
    }

    /// A cancelled controller with durable history ⇒ "canceled".
    #[tokio::test]
    async fn infer_status_reports_canceled_for_cancelled_controller() {
        let sink = synthia_session::in_memory::InMemorySessionSink::new("s1");
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
}
