//! WebSocket types

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct WsMessage {
    pub action: String,
    pub content: Option<String>,
}
