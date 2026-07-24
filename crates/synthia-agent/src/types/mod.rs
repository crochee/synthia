//! Core runtime types for synthia-agent
//!
//! This module contains core event and notification types used during
//! agent execution. These types represent runtime events and status
//! information, NOT persistent data models.
//!
//! ## Type Categories
//!
//! - **Agent Events**: Runtime events emitted during agent execution
//! - **Agent Status**: Current state of an agent instance
//! - **System Notifications**: Messages sent to the user interface
//!
//! For persistent data models (Session, Task, Memory, etc.), see
//! [`crate::storage::types`].

mod notification;
pub mod team_types;

use chrono::{DateTime, Utc};
pub use notification::{SystemNotification, SystemNotificationType};
use serde::{Deserialize, Serialize};

pub use crate::{
    config::{AgentConfig, AgentConfigBuilder},
    events::{AgentEvent, AgentOutput, SessionEndReason, TokenUsage},
    input::AgentInput,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflection {
    pub iteration: usize,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

impl Reflection {
    pub fn new(
        iteration: usize,
        summary: String,
        issues: Vec<String>,
        suggestions: Vec<String>,
    ) -> Self {
        Self {
            iteration,
            timestamp: Utc::now(),
            summary,
            issues,
            suggestions,
        }
    }
}

pub const REFLECTION_INTERVAL: usize = 5;
pub const REFLECTION_TOKEN_BUDGET_PERCENTAGE: f64 = 0.1;

pub use synthia_provider::types::SamplingResult;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_name: String,
    pub output: String,
    pub is_error: bool,
    /// The `ToolUse.id` from the original `CompletionResponse` that
    /// produced this result. Used to construct a `Message::tool(..., id)`
    /// so the next LLM call can correlate the result with the
    /// `Role::Assistant` message that requested it. Without this, the
    /// tool result is unreachable to the LLM (Anthropic/OpenAI both
    /// require a matching `tool_call_id` on the `Role::Tool` message).
    pub tool_call_id: String,
}
