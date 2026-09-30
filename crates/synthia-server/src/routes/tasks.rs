//! `task` delegation HTTP surface — replaces the N+1 fan-out
//! the frontend's `tasksApi.listAll` was doing by scanning
//! recent sessions server-side and returning one flat list of
//! `TaskDelegation` records.
//!
//! The `task` tool is model-facing (`synthia-tool-task`); since
//! delegated children run in their **own sessions** (see
//! `crate::session::subagent_router`), a delegation surfaces in
//! the parent's log as a pair of lightweight marker rows —
//! `{ type: "subagent_enter", data: { child_session_id,
//! parent_session_id, depth, agent_name } }` and the matching
//! `subagent_exit` carrying the terminal status. The child's
//! prompt lives in the child session's own log (its `UserInput`
//! row), which this route reads for the prompt column.
//!
//! Surface:
//! - `GET /api/v1/tasks`             — list all delegations across recent sessions
//! - `GET /api/v1/tasks/{id}`        — fetch one delegation by `sessionId:eventSeq`

use std::sync::Arc;

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    api::{AppError, AppPath, AppQuery, List, PageQuery, resolve_page},
    state::AppState,
};

/// `TaskDelegation` — one delegation, flattened for the frontend.
/// Wire shape mirrors `synthia-web/src/api/types.ts::TaskDelegation`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDelegation {
    /// Synthetic id `<sessionId>:<eventSeq>` — survives a refresh.
    pub id: String,
    pub parent_session_id: String,
    pub child_session_id: Option<String>,
    pub parent_depth: u32,
    pub agent: String,
    pub prompt: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub status: String,
}

/// Cap on the fan-out: how many recent sessions to scan. Keeps a
/// workspace with thousands of sessions from exhausting the
/// request budget from this page.
const TASK_SCAN_LIMIT: usize = 50;

/// `GET /api/v1/tasks` — server-side scan.
pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
    AppQuery(page): AppQuery<PageQuery>,
) -> Result<Json<List<TaskDelegation>>, AppError> {
    let _ = resolve_page(&page)?;
    let user_id = state.default_user_id().to_string();
    let delegations =
        scan_recent_sessions(&state, &user_id, TASK_SCAN_LIMIT).await;
    let total = delegations.len() as u64;
    Ok(Json(List {
        data: delegations,
        next_cursor: None,
        total: Some(total),
    }))
}

/// `GET /api/v1/tasks/{id}` — fetch one delegation by synthetic id.
///
/// The id addresses the `subagent_enter` marker row in the parent
/// session's log; the matching `subagent_exit` (same child id, if
/// the delegation already finished) supplies the terminal status.
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Json<TaskDelegation>, AppError> {
    let (session_id, event_seq) = parse_task_id(&id)?;
    let user_id = state.default_user_id().to_string();
    let sink = state.session_manager.sink(&user_id, &session_id);
    let raw = sink.read().await.map_err(|e| {
        AppError::from(synthia::core::Error::session(format!("{e}")))
    })?;
    let row = raw.get(event_seq).ok_or_else(|| {
        AppError::from(synthia::core::Error::not_found(format!(
            "event {event_seq} in session '{session_id}'"
        )))
    })?;
    if row.get("type").and_then(Value::as_str) != Some("subagent_enter") {
        return Err(AppError::from(synthia::core::Error::not_found(format!(
            "event {event_seq} in session '{session_id}' is not a sub-agent delegation"
        ))));
    }
    let child_id = row
        .pointer("/data/child_session_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::from(synthia::core::Error::validation(format!(
                "event {event_seq} in session '{session_id}' is malformed"
            )))
        })?
        .to_string();
    let exit = find_exit(&raw, &child_id);
    let prompt = child_prompt(&state, &user_id, &child_id).await;
    Ok(Json(to_delegation(
        &session_id,
        event_seq,
        row,
        exit.as_ref(),
        &prompt,
    )))
}

/// Walk the most recent `limit` sessions and extract every
/// delegation from its `subagent_enter` / `subagent_exit` marker
/// pairs. Sessions that fail to load are silently skipped — a
/// partial result is better than nothing on this read-only page.
pub(crate) async fn scan_recent_sessions(
    state: &AppState,
    user_id: &str,
    limit: usize,
) -> Vec<TaskDelegation> {
    let sessions = match state.session_manager.list_all().await {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<TaskDelegation> = Vec::new();
    for session in sessions.into_iter().take(limit) {
        let sink = state.session_manager.sink(user_id, &session.id);
        let raw = match sink.read().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        // (child id → index into `out`), so each exit finds its enter.
        let mut pending: Vec<(String, usize)> = Vec::new();
        for (idx, row) in raw.iter().enumerate() {
            match row.get("type").and_then(Value::as_str) {
                Some("subagent_enter") => {
                    let Some(child_id) = row
                        .pointer("/data/child_session_id")
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let child_id = child_id.to_string();
                    let prompt = child_prompt(state, user_id, &child_id).await;
                    out.push(to_delegation(
                        &session.id,
                        idx,
                        row,
                        None,
                        &prompt,
                    ));
                    pending.push((child_id, out.len() - 1));
                }
                Some("subagent_exit") => {
                    let Some(child_id) = row
                        .pointer("/data/child_session_id")
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    if let Some(pos) =
                        pending.iter().position(|(id, _)| id == child_id)
                    {
                        let (_, out_idx) = pending.swap_remove(pos);
                        let finished_at = row
                            .get("ts")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let status = exit_status(
                            row.pointer("/data/status").and_then(Value::as_str),
                        );
                        if let Some(entry) = out.get_mut(out_idx) {
                            entry.finished_at = finished_at;
                            entry.status = status;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The delegation's prompt: the child session's first `UserInput`
/// row. An unreadable or empty child log yields the empty string —
/// the list column degrades, the row does not disappear.
async fn child_prompt(
    state: &AppState,
    user_id: &str,
    child_id: &str,
) -> String {
    let sink = state.session_manager.sink(user_id, child_id);
    let Ok(rows) = sink.read().await else {
        return String::new();
    };
    rows.iter()
        .find(|row| {
            row.get("type").and_then(Value::as_str) == Some("UserInput")
        })
        .and_then(|row| row.pointer("/data/text").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

/// The matching `subagent_exit` row for one child, if the
/// delegation already finished.
fn find_exit(rows: &[Value], child_id: &str) -> Option<Value> {
    rows.iter()
        .find(|row| {
            row.get("type").and_then(Value::as_str) == Some("subagent_exit")
                && row
                    .pointer("/data/child_session_id")
                    .and_then(Value::as_str)
                    == Some(child_id)
        })
        .cloned()
}

/// Marker status → the wire vocabulary the frontend renders:
/// `completed` → `succeeded`, anything else keeps its own label.
fn exit_status(raw: Option<&str>) -> String {
    match raw {
        Some("completed") => "succeeded".to_string(),
        Some(other) => other.to_string(),
        None => "failed".to_string(),
    }
}

fn parse_task_id(id: &str) -> Result<(String, usize), AppError> {
    let Some((session_id, seq_str)) = id.split_once(':') else {
        return Err(AppError::from(synthia::core::Error::validation(format!(
            "invalid task id '{id}': expected <sessionId>:<eventSeq>"
        ))));
    };
    let Ok(seq) = seq_str.parse::<usize>() else {
        return Err(AppError::from(synthia::core::Error::validation(format!(
            "invalid task id '{id}': event seq is not a number"
        ))));
    };
    Ok((session_id.to_string(), seq))
}

/// Build one delegation from its `subagent_enter` row (and the
/// matching exit row when the delegation finished).
fn to_delegation(
    session_id: &str,
    event_seq: usize,
    enter: &Value,
    exit: Option<&Value>,
    prompt: &str,
) -> TaskDelegation {
    let str_at = |pointer: &str| {
        enter
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    TaskDelegation {
        id: format!("{session_id}:{event_seq}"),
        parent_session_id: str_at("/data/parent_session_id")
            .unwrap_or_else(|| session_id.to_string()),
        child_session_id: str_at("/data/child_session_id"),
        parent_depth: enter
            .pointer("/data/depth")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        agent: str_at("/data/agent_name").unwrap_or_else(|| "unknown".into()),
        prompt: prompt.to_string(),
        started_at: enter.get("ts").and_then(Value::as_str).map(str::to_string),
        finished_at: exit
            .and_then(|row| row.get("ts").and_then(Value::as_str))
            .map(str::to_string),
        status: match exit {
            Some(row) => {
                exit_status(row.pointer("/data/status").and_then(Value::as_str))
            }
            None => "running".to_string(),
        },
    }
}
