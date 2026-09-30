//! `synthia-eval` HTTP surface — in-memory `EvalSuite` registry
//! plus a keyword-only runner.
//!
//! Surface:
//! - `GET    /api/v1/evals`         — list suites
//! - `GET    /api/v1/evals/{name}`  — single suite
//! - `POST   /api/v1/evals`         — register a suite
//! - `DELETE /api/v1/evals/{name}`  — drop a suite
//! - `POST   /api/v1/evals/{name}/run` — run the suite, return report

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use synthia::eval::{EvalReport, EvalSuite};

use crate::{
    api::{AppError, AppPath, AppQuery, List, PageQuery, resolve_page},
    state::AppState,
};

/// `GET /api/v1/evals` — list suites.
pub async fn list_evals(
    State(state): State<Arc<AppState>>,
    AppQuery(page): AppQuery<PageQuery>,
) -> Result<Json<List<EvalSuite>>, AppError> {
    let _ = resolve_page(&page)?;
    let suites = state.evals.list().await;
    let total = suites.len() as u64;
    Ok(Json(List {
        data: suites,
        next_cursor: None,
        total: Some(total),
    }))
}

/// `GET /api/v1/evals/{name}` — single suite detail.
pub async fn get_eval(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<Json<EvalSuite>, AppError> {
    let suite = state.evals.get(&name).await.ok_or_else(|| {
        AppError::from(synthia::core::Error::not_found(format!(
            "eval '{name}'"
        )))
    })?;
    Ok(Json(suite))
}

/// `POST /api/v1/evals` — register a suite.
pub async fn create_eval(
    State(state): State<Arc<AppState>>,
    axum::Json(req): axum::Json<EvalSuite>,
) -> Result<(StatusCode, Json<EvalSuite>), AppError> {
    let name = req.name().to_string();
    state
        .evals
        .create(req)
        .await
        .map_err(|e| AppError::from(synthia::core::Error::already_exists(e)))?;
    let suite = state.evals.get(&name).await.ok_or_else(|| {
        AppError::from(synthia::core::Error::internal(
            "created eval suite disappeared",
        ))
    })?;
    Ok((StatusCode::CREATED, Json(suite)))
}

/// `DELETE /api/v1/evals/{name}` — drop a suite.
pub async fn delete_eval(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<StatusCode, AppError> {
    if state.evals.remove(&name).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::from(synthia::core::Error::not_found(format!(
            "eval '{name}'"
        ))))
    }
}

/// `POST /api/v1/evals/{name}/run` — run the suite.
pub async fn run_eval(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<Json<EvalReport>, AppError> {
    let report = state
        .evals
        .run(&name)
        .await
        .map_err(|e| AppError::from(synthia::core::Error::not_found(e)))?;
    Ok(Json(report))
}
