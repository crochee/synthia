//! Session HTTP handlers
//!
//! Handlers for session management using SessionService.

mod service;
mod types;

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
pub use service::SessionService;
pub use types::{CompactionResult, FormattedMessage, SessionInfo};

use crate::{AppState, PagedResponse, error::ServerError};

#[derive(Deserialize)]
pub struct ListSessionsQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub mark: Option<String>,
}

fn default_limit() -> usize {
    20
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<PagedResponse<SessionInfo>>, ServerError> {
    let service = SessionService::new(Arc::new(state));
    let (sessions, next_mark, has_more) =
        service.list(query.limit, query.mark.as_deref()).await?;
    Ok(Json(PagedResponse::new(sessions, next_mark, has_more)))
}

pub async fn create_session(
    State(state): State<AppState>,
) -> Result<Json<SessionInfo>, ServerError> {
    let service = SessionService::new(Arc::new(state));
    let session = service.create().await?;
    Ok(Json(session))
}

pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionInfo>, ServerError> {
    let service = SessionService::new(Arc::new(state));
    let session = service
        .get(&session_id)
        .await?
        .ok_or_else(|| ServerError::not_found("Session", &session_id))?;
    Ok(Json(session))
}

pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, ServerError> {
    let service = SessionService::new(Arc::new(state));
    service.delete(&session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn compact_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<CompactionResult>, ServerError> {
    let service = SessionService::new(Arc::new(state));
    let result = service.compact(&session_id).await?;
    Ok(Json(result))
}

pub async fn get_session_messages(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, ServerError> {
    let service = SessionService::new(Arc::new(state));
    let messages = service.get_messages(&session_id).await?;

    let result: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|msg| {
            serde_json::json!({
                "role": msg.role,
                "content": msg.content,
            })
        })
        .collect();

    Ok(Json(result))
}
