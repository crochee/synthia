//! A2A task management handlers.
//!
//! [`list_tasks`] enumerates tasks recorded by the in-memory task store
//! via the shared `DefaultRequestHandler`, supporting cursor pagination
//! and filtering by `status` / `context_id`.
//!
//! [`get_task`] fetches a single task with its history and artifacts.

use std::sync::Arc;

use a2a::{Artifact, GetTaskRequest, ListTasksRequest, Message, TaskState};
use a2a_server::{RequestHandler, ServiceParams};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::helpers::paginate;
use crate::{
    api::{
        ErrorCode,
        List,
        MAX_LIMIT,
        TaskPageQuery,
        UserError,
        decode_cursor,
        resolve_page,
        validate_resource_name,
        validate_sort,
    },
    state::AppState,
};

/// Sortable fields for the tasks list endpoint.
///
/// NOTE: `api-list-pagination/spec.md:80` lists `tasks (created_at,
/// updated_at, status)`, but `updated_at` is intentionally omitted
/// here. The list endpoint builds [`TaskSummary`] from the A2A `Task`
/// type, which does not expose an `updated_at` field on list items
/// (only [`TaskDetail`] has it, derived from `status.timestamp`).
/// Accepting `?sort=updated_at` would silently fall back to
/// `created_at` and mislead clients. The spec will be updated in a
/// follow-up to drop `updated_at` from the tasks sort whitelist.
const TASK_SORT_WHITELIST: &[&str] = &["created_at", "status"];

/// Frontend-facing task summary (matches the TypeScript `TaskSummary` type).
#[derive(Serialize)]
pub struct TaskSummary {
    pub id: String,
    /// Static string slice — see `task_state_label` for why this
    /// isn't `String`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

/// Detailed task view including history and artifacts.
///
/// Per `api-management-routes/spec.md:77`: TaskDetail must include
/// `id, status, context_id, created_at, updated_at, history, artifacts`.
#[derive(Serialize)]
pub struct TaskDetail {
    pub id: String,
    /// Static string slice — see `task_state_label` for why this
    /// isn't `String`.
    pub status: &'static str,
    pub context_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    pub history: Vec<Message>,
    pub artifacts: Vec<Artifact>,
}

/// Validate a status filter string against the A2A TaskState labels.
fn parse_status_filter(status: &str) -> Result<TaskState, UserError> {
    match status {
        "unspecified" => Ok(TaskState::Unspecified),
        "submitted" => Ok(TaskState::Submitted),
        "working" => Ok(TaskState::Working),
        "completed" => Ok(TaskState::Completed),
        "failed" => Ok(TaskState::Failed),
        "canceled" => Ok(TaskState::Canceled),
        "input_required" => Ok(TaskState::InputRequired),
        "rejected" => Ok(TaskState::Rejected),
        "auth_required" => Ok(TaskState::AuthRequired),
        _ => Err(UserError::new(
            ErrorCode::BadRequest,
            format!("invalid status filter: {:?}", status),
        )),
    }
}

/// Convert an A2A [`TaskState`] to its lowercase wire label.
///
/// Returns `&'static str` (not `String`) — the labels are a closed
/// set of 9 values, so each list page used to allocate 9 ×
/// `MAX_LIMIT` redundant `String`s. Switching to static slices
/// drops that to zero allocations in the hot path; the field
/// type on `TaskSummary` is unchanged because `&'static str`
/// serialises identically to `String`.
fn task_state_label(state: &TaskState) -> &'static str {
    match state {
        TaskState::Unspecified => "unspecified",
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
        TaskState::Canceled => "canceled",
        TaskState::InputRequired => "input_required",
        TaskState::Rejected => "rejected",
        TaskState::AuthRequired => "auth_required",
    }
}

/// GET /api/tasks - List A2A tasks with cursor pagination + filters.
pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TaskPageQuery>,
) -> Result<Json<List<TaskSummary>>, UserError> {
    validate_sort(
        query.page.sort.as_deref().unwrap_or("-created_at"),
        TASK_SORT_WHITELIST,
    )?;
    let resolved = resolve_page(&query.page)?;

    let status_filter = match query.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(parse_status_filter(s)?),
    };

    // The v1 cursor is an opaque base64(id) so a client that
    // sends `?cursor=…` can resume after a specific task.
    // Decode it BEFORE handing off to the A2A handler so the
    // A2A layer sees a plain task id (`page_token`) instead of
    // an opaque blob. If the cursor is missing this returns
    // `None` and the handler falls back to page 0.
    //
    // Without this decoding the A2A handler always saw
    // `page_token: None` and re-read the store from index 0,
    // so any client that paged with `loadMore()` would see
    // duplicate ids on the next request.
    let page_token = match query.page.cursor.as_deref() {
        None | Some("") => None,
        Some(cursor) => Some(decode_cursor(cursor)?),
    };

    let a2a = state.a2a_service(None).await;
    let handler = a2a.handler_arc();

    let req = ListTasksRequest {
        context_id: query.context_id.clone(),
        status: status_filter,
        page_size: Some(MAX_LIMIT as i32),
        page_token,
        history_length: Some(0),
        status_timestamp_after: None,
        include_artifacts: Some(false),
        tenant: None,
    };
    let params = ServiceParams::new();

    let response = handler.list_tasks(&params, req).await.map_err(|e| {
        UserError::new(
            ErrorCode::InternalServerError,
            format!("failed to list A2A tasks: {e}"),
        )
    })?;

    let mut tasks: Vec<TaskSummary> = response
        .tasks
        .into_iter()
        .map(|task| TaskSummary {
            id: task.id,
            status: task_state_label(&task.status.state),
            context_id: Some(task.context_id),
            created_at: task.status.timestamp,
        })
        .collect();

    let field = resolved.sort_field.as_deref().unwrap_or("created_at");
    match field {
        "status" => {
            tasks.sort_by(|a, b| a.status.cmp(b.status));
            if resolved.descending {
                tasks.reverse();
            }
        }
        _ => {
            tasks.sort_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.id.cmp(&b.id))
            });
            if resolved.descending {
                tasks.reverse();
            }
        }
    }

    let list = paginate(tasks, &resolved, |t: &TaskSummary| t.id.as_str());
    Ok(Json(list))
}

/// GET /api/tasks/{id} - Get a single task with history and artifacts.
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TaskDetail>, UserError> {
    validate_resource_name(&id)?;
    let a2a = state.a2a_service(None).await;
    let handler = a2a.handler_arc();

    let req = GetTaskRequest {
        id: id.clone(),
        history_length: None,
        tenant: None,
    };
    let params = ServiceParams::new();

    let task = handler.get_task(&params, req).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not found") {
            UserError::new(
                ErrorCode::NotFound,
                format!("Task '{}' not found", id),
            )
        } else {
            UserError::new(
                ErrorCode::InternalServerError,
                format!("failed to get task: {e}"),
            )
        }
    })?;

    // The A2A `Task` type exposes only `status.timestamp` (the time
    // the current status was recorded). There is no dedicated
    // `created_at` field, and `Message` carries no timestamp either,
    // so we fall back to `status.timestamp` for `created_at`. This
    // mirrors the `TaskSummary` mapping and means `created_at` and
    // `updated_at` may coincide until the A2A SDK exposes a real
    // creation timestamp.
    let status_ts = task.status.timestamp;
    Ok(Json(TaskDetail {
        id: task.id,
        status: task_state_label(&task.status.state),
        context_id: task.context_id,
        created_at: status_ts,
        updated_at: status_ts,
        history: task.history.unwrap_or_default(),
        artifacts: task.artifacts.unwrap_or_default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ErrorCode;

    // --- parse_status_filter ---

    #[test]
    fn parse_status_filter_accepts_all_valid_states() {
        for (label, expected) in [
            ("unspecified", TaskState::Unspecified),
            ("submitted", TaskState::Submitted),
            ("working", TaskState::Working),
            ("completed", TaskState::Completed),
            ("failed", TaskState::Failed),
            ("canceled", TaskState::Canceled),
            ("input_required", TaskState::InputRequired),
            ("rejected", TaskState::Rejected),
            ("auth_required", TaskState::AuthRequired),
        ] {
            let got =
                parse_status_filter(label).expect("valid label should parse");
            assert_eq!(got, expected, "label {label:?} mismatch");
        }
    }

    #[test]
    fn parse_status_filter_rejects_unknown_label() {
        let err =
            parse_status_filter("not_a_state").expect_err("unknown label");
        assert_eq!(err.code, ErrorCode::BadRequest);
        assert!(err.message.contains("invalid status filter"));
    }

    #[test]
    fn parse_status_filter_is_case_sensitive() {
        // Wire labels are lowercase snake_case; uppercase variants
        // are rejected so clients get a stable contract.
        let err = parse_status_filter("Completed").expect_err("uppercase");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    // --- task_state_label ---

    #[test]
    fn task_state_label_maps_every_variant() {
        for (state, expected) in [
            (TaskState::Unspecified, "unspecified"),
            (TaskState::Submitted, "submitted"),
            (TaskState::Working, "working"),
            (TaskState::Completed, "completed"),
            (TaskState::Failed, "failed"),
            (TaskState::Canceled, "canceled"),
            (TaskState::InputRequired, "input_required"),
            (TaskState::Rejected, "rejected"),
            (TaskState::AuthRequired, "auth_required"),
        ] {
            assert_eq!(
                task_state_label(&state),
                expected,
                "state {:?} should map to {expected:?}",
                state,
            );
        }
    }

    #[test]
    fn task_state_label_roundtrips_with_parse_status_filter() {
        // Every label produced by task_state_label must be accepted
        // by parse_status_filter (and map back to the same variant).
        for state in [
            TaskState::Unspecified,
            TaskState::Submitted,
            TaskState::Working,
            TaskState::Completed,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::InputRequired,
            TaskState::Rejected,
            TaskState::AuthRequired,
        ] {
            let label = task_state_label(&state);
            let parsed = parse_status_filter(label).expect("roundtrip parse");
            assert_eq!(parsed, state, "roundtrip failed for {label:?}");
        }
    }
}
