//! Chat types for API requests and responses

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub session_id: Option<String>,
    /// Agent name to use (defaults to "code")
    #[serde(default)]
    pub agent: Option<String>,
}

/// Tool progress record for tracking
#[derive(Debug, Serialize)]
pub struct ToolProgress {
    pub tool: String,
    pub progress: String,
}

/// Detailed chat response with tool progress
#[derive(Debug, Serialize)]
pub struct DetailedChatResponse {
    pub message: String,
    pub session_id: String,
    pub tool_progress: Vec<ToolProgress>,
    pub agent: String,
}
