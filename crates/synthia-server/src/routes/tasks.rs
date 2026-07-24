//! GET /api/tasks - List A2A tasks recorded by the in-memory task store.
//!
//! Returns a `{tasks: [...]}` envelope compatible with the
//! synthia-web `TasksPage` consumer (which expects `TaskSummary`
//! objects with `id`, `status`, optional `contextId` and `createdAt`).
//!
//! Tasks are enumerated via the shared `DefaultRequestHandler` so that
//! tasks created via JSON-RPC / REST are visible to this management
//! endpoint without an extra round-trip.

use std::sync::Arc;

use a2a::{ListTasksRequest, TaskState};
use a2a_server::{RequestHandler, ServiceParams};
use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::Serialize;
use synthia_core::{ApiResponse, ErrorCode, UserError};

use crate::state::AppState;

/// Frontend-facing task summary (matches the TypeScript `TaskSummary` type).
#[derive(Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskSummary>,
    pub count: usize,
}

/// GET /api/tasks - List A2A tasks via the shared request handler.
pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let a2a = state.a2a_service("".to_string()).await;
    let handler = a2a.handler_arc();

    let req = ListTasksRequest {
        context_id: None,
        status: None,
        page_size: None,
        page_token: None,
        history_length: None,
        status_timestamp_after: None,
        include_artifacts: None,
        tenant: None,
    };
    let params = ServiceParams::new();

    let response = match handler.list_tasks(&params, req).await {
        Ok(resp) => resp,
        Err(e) => {
            return Json(ApiResponse::err(UserError::new(
                ErrorCode::InternalServerError,
                format!("failed to list A2A tasks: {e}"),
            )));
        }
    };

    let tasks: Vec<TaskSummary> = response
        .tasks
        .into_iter()
        .map(|task| TaskSummary {
            id: task.id,
            status: task_state_label(&task.status.state),
            context_id: Some(task.context_id),
            created_at: task.status.timestamp,
        })
        .collect();

    let payload = TaskListResponse {
        count: tasks.len(),
        tasks,
    };

    let value = serde_json::to_value(payload)
        .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }));

    Json(ApiResponse::ok(value))
}

fn task_state_label(state: &TaskState) -> String {
    match state {
        TaskState::Unspecified => "unspecified".to_string(),
        TaskState::Submitted => "submitted".to_string(),
        TaskState::Working => "working".to_string(),
        TaskState::Completed => "completed".to_string(),
        TaskState::Failed => "failed".to_string(),
        TaskState::Canceled => "canceled".to_string(),
        TaskState::InputRequired => "input_required".to_string(),
        TaskState::Rejected => "rejected".to_string(),
        TaskState::AuthRequired => "auth_required".to_string(),
    }
}
