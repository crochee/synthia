//! WebSocket HTTP handlers
//!
//! Handler for WebSocket chat.

mod types;

use axum::extract::{Path, State, ws::WebSocketUpgrade};
use futures::{SinkExt, StreamExt};
use synthia_agent::{config::SessionConfig, AgentEvent};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
pub use types::WsMessage;

use crate::{
    AppState,
    error::ServerError,
    utils::{create_user_message, extract_text},
};

fn format_event(event: AgentEvent) -> Option<String> {
    match event {
        AgentEvent::Message(msg) => {
            let text = extract_text(&msg).unwrap_or_default();
            Some(
                serde_json::json!({
                    "type": "message",
                    "content": text,
                })
                .to_string(),
            )
        }
        AgentEvent::Status(status) => Some(
            serde_json::json!({
                "type": "status",
                "status": format!("{:?}", status),
            })
            .to_string(),
        ),
        _ => None,
    }
}

fn format_error(error: impl std::fmt::Display) -> String {
    serde_json::json!({
        "type": "error",
        "error": error.to_string(),
    })
    .to_string()
}

pub async fn websocket(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<axum::response::Response, ServerError> {
    let session_config = SessionConfig {
        id: session_id,
        ..Default::default()
    };

    Ok(ws
        .on_upgrade(move |socket| handle_socket(socket, state, session_config)))
}

async fn handle_socket(
    socket: axum::extract::ws::WebSocket,
    state: AppState,
    session_config: SessionConfig,
) {
    use axum::extract::ws::Message;

    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if sender.send(Message::Text(data.into())).await.is_err() {
                break;
            }
        }
    });

    let agent = state.agent.clone();
    let cancel_token = CancellationToken::new();

    while let Some(msg) = receiver.next().await {
        if let Ok(Message::Text(text)) = msg
            && let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text)
        {
            match ws_msg.action.as_str() {
                "chat" => {
                    handle_chat_action(
                        &agent,
                        &session_config,
                        ws_msg.content.unwrap_or_default(),
                        cancel_token.clone(),
                        &tx,
                    )
                    .await;
                }
                "cancel" => {
                    cancel_token.cancel();
                }
                _ => {}
            }
        }
    }
}

async fn handle_chat_action(
    agent: &synthia_agent::Agent,
    session_config: &SessionConfig,
    content: String,
    cancel_token: CancellationToken,
    tx: &mpsc::UnboundedSender<String>,
) {
    let user_msg = create_user_message(content);
    let stream = agent.reply(user_msg, session_config, cancel_token).await;

    match stream {
        Ok(mut stream) => {
            while let Some(event_result) = stream.next().await {
                match event_result {
                    Ok(event) => {
                        if let Some(formatted) = format_event(event) {
                            let _ = tx.send(formatted);
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(format_error(e));
                    }
                }
            }
        }
        Err(e) => {
            let _ = tx.send(format_error(e));
        }
    }
}
