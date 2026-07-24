use std::{sync::Arc, time::Duration};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use synthia_core::{ApiResponse, ErrorCode, UserError};
use synthia_job::{Trigger, parse_standard};

use crate::state::AppState;

#[derive(Deserialize)]
#[serde(tag = "trigger_type", rename_all = "snake_case")]
pub(crate) enum TriggerConfig {
    Cron { expression: String },
    Interval { seconds: u64 },
    Once { delay_seconds: u64 },
    At { timestamp: i64 },
}

fn build_trigger(config: TriggerConfig) -> Result<Box<dyn Trigger>, String> {
    match config {
        TriggerConfig::Cron { expression } => {
            parse_standard(&expression).map_err(|e| e.to_string())
        }
        TriggerConfig::Interval { seconds } => {
            Ok(Box::new(synthia_job::every(Duration::from_secs(seconds))))
        }
        TriggerConfig::Once { delay_seconds } => Ok(Box::new(
            synthia_job::run_once(Duration::from_secs(delay_seconds)),
        )),
        TriggerConfig::At { timestamp } => {
            let ns = timestamp * 1_000_000_000;
            Ok(Box::new(synthia_job::run_at(ns)))
        }
    }
}

#[derive(Deserialize)]
pub struct ScheduleJobRequest {
    key: String,
    #[serde(flatten)]
    trigger_config: TriggerConfig,
}

#[derive(Deserialize, Serialize)]
struct JobInfo {
    key: String,
    description: String,
    trigger_desc: String,
}

#[derive(Deserialize)]
pub struct JobFilterQuery {
    key: Option<String>,
    trigger_contains: Option<String>,
}

pub async fn schedule_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScheduleJobRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    if req.key.is_empty() {
        return Json(ApiResponse::err(UserError::new(
            ErrorCode::BadRequest,
            "job key must not be empty",
        )));
    }

    let trigger = match build_trigger(req.trigger_config) {
        Ok(t) => t,
        Err(e) => {
            return Json(ApiResponse::err(UserError::new(
                ErrorCode::BadRequest,
                format!("invalid trigger configuration: {e}"),
            )));
        }
    };

    match state
        .job_scheduler
        .schedule(&req.key, Arc::from(trigger))
        .await
    {
        Ok(()) => Json(ApiResponse::ok(serde_json::json!({
            "status": "scheduled",
            "key": req.key
        }))),
        Err(e) => {
            if e.to_string().contains("paused") {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::BadRequest,
                    format!("job '{}' is paused", req.key),
                )))
            } else if e.to_string().contains("not found") {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::BadRequest,
                    format!("job '{}' is not registered", req.key),
                )))
            } else if e.to_string().contains("already exists") {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::Conflict,
                    format!("job '{}' is already scheduled", req.key),
                )))
            } else {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::InternalServerError,
                    format!("failed to schedule job: {e}"),
                )))
            }
        }
    }
}

pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
    Query(filter): Query<JobFilterQuery>,
) -> Json<ApiResponse<serde_json::Value>> {
    let scheduled = state.job_scheduler.time_wheel().jobs_with_filter(
        filter.key.as_deref(),
        filter.trigger_contains.as_deref(),
    );

    let paused = state.job_scheduler.list_paused();

    let jobs: Vec<JobInfo> = scheduled
        .into_iter()
        .map(|sj| JobInfo {
            key: sj.job.key().to_string(),
            description: sj.job.description().to_string(),
            trigger_desc: sj.trigger_desc,
        })
        .collect();

    Json(ApiResponse::ok(serde_json::json!({
        "jobs": jobs,
        "paused": paused,
        "count": jobs.len(),
    })))
}

pub async fn remove_job(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.job_scheduler.remove(&key).await {
        Ok(()) => Json(ApiResponse::ok(serde_json::json!({
            "status": "removed",
            "key": key
        }))),
        Err(e) => {
            if e.to_string().contains("not found") {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::NotFound,
                    format!("job '{}' not found", key),
                )))
            } else {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::InternalServerError,
                    format!("failed to remove job: {e}"),
                )))
            }
        }
    }
}

pub async fn execute_job(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.job_scheduler.execute(&key).await {
        Ok(()) => Json(ApiResponse::ok(serde_json::json!({
            "status": "executed",
            "key": key
        }))),
        Err(e) => {
            if e.to_string().contains("not found") {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::NotFound,
                    format!("job '{}' not found", key),
                )))
            } else {
                Json(ApiResponse::err(UserError::new(
                    ErrorCode::InternalServerError,
                    format!("failed to execute job: {e}"),
                )))
            }
        }
    }
}

pub async fn toggle_pause(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if state.job_scheduler.is_paused(&key) {
        state.job_scheduler.unmark_paused(&key);

        match state.job_scheduler.registry().lookup(&key) {
            Some(_) => Json(ApiResponse::ok(serde_json::json!({
                "status": "resumed",
                "key": key
            }))),
            None => Json(ApiResponse::err(UserError::new(
                ErrorCode::NotFound,
                format!("job '{}' not found", key),
            ))),
        }
    } else {
        let registry = state.job_scheduler.registry();
        let job = registry.lookup(&key);

        match job {
            Some(_job) => {
                state.job_scheduler.mark_paused(&key);
                let _ = state.job_scheduler.remove(&key).await;

                Json(ApiResponse::ok(serde_json::json!({
                    "status": "paused",
                    "key": key
                })))
            }
            None => Json(ApiResponse::err(UserError::new(
                ErrorCode::NotFound,
                format!("job '{}' not found", key),
            ))),
        }
    }
}
