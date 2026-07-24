use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use synthia_core::{ApiResponse, Registry, RegistryItem};

use crate::{middleware::auth::RequestUserId, state::AppState};

#[derive(Serialize)]
pub struct SessionListItem {
    pub id: String,
    pub state: String,
}

#[derive(Serialize)]
pub struct SessionDetail {
    pub id: String,
    pub state: String,
    pub model: String,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

#[derive(Serialize)]
pub struct MessageItem {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
) -> Json<ApiResponse<Vec<SessionListItem>>> {
    let summaries = state.session_manager.list_for_user(user_id.as_str()).await;

    let items: Vec<SessionListItem> = summaries
        .into_iter()
        .map(|s| SessionListItem {
            id: s.id,
            state: format!("{:?}", s.state),
        })
        .collect();

    Json(ApiResponse::ok(items))
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
) -> Response {
    let session_id = synthia_core::generate_session_id();
    match state
        .session_manager
        .create_with_user(session_id.clone(), user_id.as_str().to_string())
        .await
    {
        Ok(_) => {
            tracing::info!(session_id = %session_id, user_id = %user_id.as_str(), "Session created");
            Json(ApiResponse::ok(CreateSessionResponse { session_id }))
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "error": {
                    "code": "create_failed",
                    "message": e.to_string(),
                }
            })),
        )
            .into_response(),
    }
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
) -> Response {
    match state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
    {
        Ok(session) => Json(ApiResponse::ok(SessionDetail {
            id: session.id,
            state: format!("{:?}", session.state),
            model: session.config.model,
        }))
        .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "status": "error",
                "error": {
                    "code": "not_found",
                    "message": format!("Session '{}' not found", session_id),
                }
            })),
        )
            .into_response(),
    }
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
) -> Response {
    match state
        .session_manager
        .delete_for_user(user_id.as_str(), &session_id)
        .await
    {
        Ok(()) => Json(serde_json::json!({
            "status": "ok",
            "data": {
                "deleted": true,
                "session_id": session_id
            }
        }))
        .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "status": "error",
                "error": {
                    "code": "not_found",
                    "message": format!("Session '{}' not found", session_id),
                }
            })),
        )
            .into_response(),
    }
}

pub async fn get_session_messages(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
) -> Response {
    match state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
    {
        Ok(_) => {
            let messages = state
                .session_manager
                .store()
                // user_id is resolved by the auth middleware from the
                // request's API key (or `SERVER_DEFAULT_USER_ID` if no
                // key is configured) and surfaced via RequestUserId.
                .load_messages_all(user_id.as_str(), &session_id)
                .unwrap_or_default();
            let items: Vec<MessageItem> = messages
                .iter()
                .map(|m: &synthia_provider::Message| MessageItem {
                    role: format!("{:?}", m.role),
                    content: match &m.content {
                        synthia_provider::Content::Single(part) => {
                            part.text().unwrap_or("").to_string()
                        }
                        synthia_provider::Content::Multi(parts) => parts
                            .iter()
                            .filter_map(|p| p.text())
                            .collect::<Vec<_>>()
                            .join(" "),
                    },
                })
                .collect();
            Json(serde_json::json!({
                "status": "ok",
                "data": {
                    "session_id": session_id,
                    "messages": items,
                    "count": items.len()
                }
            }))
            .into_response()
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "status": "error",
                "error": {
                    "code": "not_found",
                    "message": format!("Session '{}' not found", session_id),
                }
            })),
        )
            .into_response(),
    }
}

pub async fn send_message(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Response {
    match state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
    {
        Ok(_) => Json(serde_json::json!({
            "status": "ok",
            "data": {
                "session_id": session_id,
                "status": "message_received",
                "content": req.content
            }
        }))
        .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "status": "error",
                "error": {
                    "code": "not_found",
                    "message": format!("Session '{}' not found", session_id),
                }
            })),
        )
            .into_response(),
    }
}

pub async fn get_session_tools(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Response {
    let tool_reg = state.tool_registry.read().await;
    let tools: Vec<_> = tool_reg
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| synthia_provider::ToolDefinition {
                    name: e.name().to_string(),
                    description: e.description().to_string(),
                    input_schema: e.tool_instance().parameters(),
                    cache_control: None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "description": d.description,
            })
        })
        .collect();
    drop(tool_reg);

    Json(serde_json::json!({
        "status": "ok",
        "data": {
            "session_id": session_id,
            "tools": tools,
            "count": tools.len()
        }
    }))
    .into_response()
}
