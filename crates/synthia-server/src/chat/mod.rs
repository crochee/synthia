//! Chat HTTP handlers
//!
//! Handlers for regular chat and streaming chat using ChatService.

mod service;
mod types;

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    response::sse::{Event, Sse},
};
use futures::StreamExt;
pub use service::ChatService;
use synthia_agent::types::AgentEvent;
use tokio_util::sync::CancellationToken;
pub use types::{ChatRequest, ChatResponse};

use crate::{AppState, error::ServerError, utils::extract_text};

pub async fn chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, ServerError> {
    let service = ChatService::new(Arc::new(state.clone()));
    let session_id = service.get_or_create_session(req.session_id).await?;
    let session_config = service.create_session_config(session_id.clone());
    let user_msg = service.create_user_message(req.message);

    let cancel_token = CancellationToken::new();

    service.add_message(&session_id, &user_msg).await?;

    let stream = state
        .agent
        .reply(user_msg, &session_config, cancel_token)
        .await?;

    let mut full_response = String::new();
    tokio::pin!(stream);
    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(AgentEvent::Message(msg)) => {
                if let Some(text) = extract_text(&msg) {
                    full_response.push_str(&text);
                }
            }
            Ok(AgentEvent::Status(status)) => {
                if matches!(
                    status,
                    synthia_agent::types::AgentStatus::Completed
                ) {
                    break;
                }
            }
            Err(e) => {
                return Err(ServerError::AgentError(e.to_string()));
            }
            _ => {}
        }
    }

    Ok(Json(ChatResponse {
        message: full_response,
        session_id,
    }))
}

pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, axum::Error>>>,
    ServerError,
> {
    let service = ChatService::new(Arc::new(state.clone()));
    let session_id = service.get_or_create_session(req.session_id).await?;
    let session_config = service.create_session_config(session_id.clone());
    let user_msg = service.create_user_message(req.message);

    let cancel_token = CancellationToken::new();

    service.add_message(&session_id, &user_msg).await?;

    let session_id_clone = session_id.clone();
    let event_stream = async_stream::stream! {
        let stream = match state.agent.reply(user_msg, &session_config, cancel_token).await {
            Ok(s) => s,
            Err(e) => {
                let data = serde_json::json!({
                    "type": "error",
                    "error": e.to_string(),
                })
                .to_string();
                yield Ok(Event::default().data(data));
                return;
            }
        };
        let mut stream = stream;
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(AgentEvent::Message(msg)) => {
                    let text = extract_text(&msg).unwrap_or_default();
                    let data = serde_json::json!({
                        "type": "message",
                        "content": text,
                        "session_id": session_id_clone,
                    })
                    .to_string();
                    yield Ok(Event::default().data(data));
                }
                Ok(AgentEvent::Status(status)) => {
                    let done = matches!(status, synthia_agent::types::AgentStatus::Completed);
                    let data = serde_json::json!({
                        "type": "status",
                        "status": format!("{:?}", status),
                        "session_id": session_id_clone,
                    })
                    .to_string();
                    yield Ok(Event::default().data(data));
                    if done {
                        break;
                    }
                }
                Err(e) => {
                    let data = serde_json::json!({
                        "type": "error",
                        "error": e.to_string(),
                    })
                    .to_string();
                    yield Ok(Event::default().data(data));
                }
                _ => {}
            }
        }
    };

    Ok(Sse::new(event_stream))
}
