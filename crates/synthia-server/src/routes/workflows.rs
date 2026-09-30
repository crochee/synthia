//! `synthia-workflow` HTTP surface — in-memory `WorkflowSpec`
//! registry + plan validation.
//!
//! Surface:
//! - `GET    /api/v1/workflows`        — list documents
//! - `GET    /api/v1/workflows/{id}`   — single document
//! - `POST   /api/v1/workflows`        — register a document
//! - `PUT    /api/v1/workflows/{id}`   — replace a document
//! - `DELETE /api/v1/workflows/{id}`   — remove a document
//! - `POST   /api/v1/workflows/{id}/run` — execute (returns
//!   `WorkflowRunResult`; today this is a stub that validates
//!   the plan and reports the planned calls; real execution
//!   needs a `WorkflowHost` wired into the agent harness).
//!
//! The wire shapes match `synthia::workflow::WorkflowSpec` and
//! (for the run result) `synthia::workflow::result::WorkflowRun`.

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use synthia::workflow::{WorkflowRun as WorkflowRunResult, WorkflowSpec};

use crate::{
    api::{AppError, AppPath, AppQuery, List, PageQuery, resolve_page},
    state::AppState,
};

#[derive(Serialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub step_count: usize,
    pub phase_count: usize,
}

/// `GET /api/v1/workflows` — list all registered documents.
pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    AppQuery(page): AppQuery<PageQuery>,
) -> Result<Json<List<WorkflowSummary>>, AppError> {
    let _ = resolve_page(&page)?;
    let specs = state.workflows.list().await;
    let total = specs.len() as u64;
    let data = specs
        .into_iter()
        .map(|s| WorkflowSummary {
            id: s.id.clone(),
            step_count: s.steps.len(),
            phase_count: s.phases.len(),
        })
        .collect();
    Ok(Json(List {
        data,
        next_cursor: None,
        total: Some(total),
    }))
}

/// `GET /api/v1/workflows/{id}` — single document detail.
pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Json<WorkflowSpec>, AppError> {
    let spec = state.workflows.get(&id).await.ok_or_else(|| {
        AppError::from(synthia::core::Error::not_found(format!(
            "workflow '{id}'"
        )))
    })?;
    Ok(Json(spec))
}

/// `POST /api/v1/workflows` — register a new document.
pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    axum::Json(req): axum::Json<WorkflowSpec>,
) -> Result<(StatusCode, Json<WorkflowSpec>), AppError> {
    let id = req.id.clone();
    state
        .workflows
        .create(req)
        .await
        .map_err(|e| AppError::from(synthia::core::Error::already_exists(e)))?;
    let spec = state.workflows.get(&id).await.ok_or_else(|| {
        AppError::from(synthia::core::Error::internal(
            "created workflow disappeared",
        ))
    })?;
    Ok((StatusCode::CREATED, Json(spec)))
}

/// `PUT /api/v1/workflows/{id}` — replace a stored document.
pub async fn replace_workflow(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
    axum::Json(req): axum::Json<WorkflowSpec>,
) -> Result<Json<WorkflowSpec>, AppError> {
    state
        .workflows
        .replace(&id, req)
        .await
        .map_err(|e| AppError::from(synthia::core::Error::validation(e)))?;
    let spec = state.workflows.get(&id).await.ok_or_else(|| {
        AppError::from(synthia::core::Error::internal(
            "replaced workflow disappeared",
        ))
    })?;
    Ok(Json(spec))
}

/// `DELETE /api/v1/workflows/{id}` — drop a document.
pub async fn delete_workflow(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<StatusCode, AppError> {
    if state.workflows.remove(&id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::from(synthia::core::Error::not_found(format!(
            "workflow '{id}'"
        ))))
    }
}

/// `POST /api/v1/workflows/{id}/run` — execute.
///
/// Today this validates the plan and returns a synthesized
/// `WorkflowRunResult` describing what *would* run. The
/// `WorkflowHost` trait implementation that calls back into
/// the agent harness lands in a follow-up turn; until then
/// every planned call settles as `Superseded` (a placeholder
/// status indicating "the run got that far").
pub async fn run_workflow(
    State(state): State<Arc<AppState>>,
    AppPath(id): AppPath<String>,
) -> Result<Json<WorkflowRunResult>, AppError> {
    let spec = state.workflows.get(&id).await.ok_or_else(|| {
        AppError::from(synthia::core::Error::not_found(format!(
            "workflow '{id}'"
        )))
    })?;
    let plan = state.workflows.plan(&spec).await.map_err(|e| {
        AppError::from(synthia::core::Error::validation(e.to_string()))
    })?;
    let call_runs = plan
        .calls()
        .iter()
        .map(|c| synthia::workflow::CallRun {
            position: c.position,
            step_id: c.step_id.clone(),
            phase: c.phase.clone(),
            status: synthia::workflow::CallStatus::Superseded,
            text: None,
            error: Some(
                "WorkflowHost not yet wired into synthia-server — runs are stubbed".to_string(),
            ),
            gate: synthia::workflow::GateVerdict::Absent,
            winner: false,
            branch_score: None,
        })
        .collect();
    Ok(Json(WorkflowRunResult {
        run_id: format!("wf_{}", uuid::Uuid::new_v4()),
        output: None,
        calls: call_runs,
        replayed: 0,
        spawned: 0,
    }))
}
