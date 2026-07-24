//! Shared request/response DTOs for the V2 session API.

use serde::{Deserialize, Serialize};

use crate::api::Direction;

/// Request body for `POST /api/v2/sessions`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionRequest {
    pub model: Option<String>,
    pub max_iterations: Option<usize>,
    pub title: Option<String>,
}

/// Full session representation returned by create/get endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub state: String,
    pub model: String,
    pub title: Option<String>,
    pub parent_id: Option<String>,
    pub max_iterations: Option<usize>,
    pub created_at: String,
    pub updated_at: String,
}

/// Lightweight session summary used by `GET /api/v2/sessions`.
#[derive(Debug, Clone, Serialize)]
pub struct SessionSummaryResponse {
    pub id: String,
    pub state: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub updated_at: String,
}

/// Cursor value for paginating the session list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListCursor {
    pub updated_at: String,
    pub id: String,
}

/// Query parameters for `GET /api/v2/sessions`.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionListQuery {
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub direction: Direction,
}

fn default_limit() -> usize {
    20
}

/// Request body for `POST /api/v2/sessions/{id}/prompts`.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptRequest {
    pub content: String,
    pub priority: Option<u8>,
}

/// Response body for a successful prompt submission.
#[derive(Debug, Clone, Serialize)]
pub struct PromptAcceptedResponse {
    pub seq: u64,
    pub admitted: bool,
    pub state: String,
}

/// Request body for `POST /api/v2/sessions/{id}/steering`.
#[derive(Debug, Clone, Deserialize)]
pub struct SteeringRequest {
    pub content: String,
    pub priority: Option<u8>,
}

/// Response body for a successful steering submission.
#[derive(Debug, Clone, Serialize)]
pub struct SteeringAcceptedResponse {
    pub admitted: bool,
    pub state: String,
}

/// Request body for `POST /api/v2/sessions/{id}/cancel`.
#[derive(Debug, Clone, Deserialize)]
pub struct CancelRequest {
    pub reason: Option<String>,
}

/// Response body for a successful cancel request.
#[derive(Debug, Clone, Serialize)]
pub struct CancelResponse {
    pub cancelled: bool,
    pub state: String,
}

/// Query parameters for `GET /api/v2/sessions/{id}/events`.
#[derive(Debug, Clone, Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub last_seq: u64,
}

/// A single message returned by `GET /api/v2/sessions/{id}/messages`.
#[derive(Debug, Clone, Serialize)]
pub struct MessageItem {
    pub seq: u64,
    pub role: String,
    pub content: String,
}

/// Cursor value for paginating messages by sequence number.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCursor {
    pub seq: u64,
}

/// Query parameters for `GET /api/v2/sessions/{id}/messages`.
#[derive(Debug, Clone, Deserialize)]
pub struct MessagesQuery {
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub direction: Direction,
}
