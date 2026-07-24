use std::sync::Arc;

use axum::{
    Extension,
    extract::{
        Path,
        State,
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};

use crate::{middleware::auth::RequestUserId, state::AppState};

pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let state_clone = state.clone();
    let user_id_clone = user_id.0.clone();
    ws.on_upgrade(move |socket| {
        handle_websocket(socket, state_clone, session_id, user_id_clone)
    })
}

pub async fn stream_handler(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let state_clone = state.clone();
    let user_id_clone = user_id.0.clone();
    ws.on_upgrade(move |socket| {
        handle_websocket(socket, state_clone, session_id, user_id_clone)
    })
}

async fn handle_websocket(
    socket: WebSocket,
    state: Arc<AppState>,
    session_id: String,
    user_id: String,
) {
    let (tx, mut rx) = socket.split();

    if state
        .session_manager
        .get_for_user(&user_id, &session_id)
        .await
        .is_err()
    {
        let mut tx = tx;
        let _ = tx
            .send(Message::Text(
                serde_json::json!({
                    "type": "error",
                    "message": "Session not found"
                })
                .to_string()
                .into(),
            ))
            .await;
        return;
    }

    let broadcaster =
        state.get_or_create_broadcaster(&user_id, &session_id).await;

    // Forward broadcast events to the WebSocket client. The WebSocket is now
    // a pure event subscriber: agent runs are spawned through the V2 REST
    // endpoints and their events are broadcast to this channel.
    let mut event_rx = broadcaster.subscribe();
    let send_task = tokio::spawn(async move {
        let mut tx = tx;
        while let Ok(event) = event_rx.recv().await {
            let event_json = serde_json::to_string(&event).unwrap_or_default();
            if tx.send(Message::Text(event_json.into())).await.is_err() {
                break;
            }
        }
    });

    // Read client messages to keep the connection alive, but do not spawn
    // agent runs from them. Control commands should use the V2 REST endpoints.
    while let Some(Ok(msg)) = rx.next().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }

    send_task.abort();

    // Clean up broadcaster if no subscribers remain after the client leaves.
    if broadcaster.subscriber_count() == 0 {
        state.remove_broadcaster(&user_id, &session_id).await;
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_agent_event_serialization() {
        use synthia_agent::types::AgentEvent;
        let event = AgentEvent::SessionStarted {
            session_id: "test-1".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("SessionStarted"));
        assert!(json.contains("test-1"));
    }

    #[test]
    fn test_agent_event_tool_call_serialization() {
        use synthia_agent::types::AgentEvent;
        let event = AgentEvent::ToolCallStarted {
            tool_name: "read_file".to_string(),
            input: serde_json::json!({"path": "/tmp/test.txt"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ToolCallStarted"));
        assert!(json.contains("read_file"));
    }

    #[test]
    fn test_agent_event_deserialization() {
        use synthia_agent::types::AgentEvent;
        let event = AgentEvent::Thinking {
            text: "analyzing code".to_string(),
            iteration: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, AgentEvent::Thinking { .. }));
    }
}
