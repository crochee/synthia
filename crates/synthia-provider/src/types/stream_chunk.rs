//! The streaming types: [`StreamChunk`] (one event on the SSE
//! stream) and [`SamplingResult`] (the final aggregated response).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{content::ContentPart, models::TokenUsage, tool::ToolUse};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SamplingResult {
    pub text: String,
    pub tool_calls: Vec<ToolUse>,
    pub reasoning: String,
    /// Anthropic `signature_delta` value attached to the most recent
    /// reasoning block, propagated so the agent can preserve
    /// cross-turn reasoning continuity.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning_signature: Option<String>,
    pub usage: TokenUsage,
}

#[derive(Clone, Debug)]
pub enum StreamChunk {
    Content(ContentPart),
    Usage(TokenUsage),
    Stop(String),
    ToolCallStart {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolCallDelta {
        id: String,
        arguments_delta: String,
    },
    ToolCallEnd {
        id: String,
    },
    IsDone {
        result: Box<SamplingResult>,
    },
}

impl From<ContentPart> for StreamChunk {
    fn from(part: ContentPart) -> Self {
        StreamChunk::Content(part)
    }
}

impl From<String> for StreamChunk {
    fn from(stop: String) -> Self {
        StreamChunk::Stop(stop)
    }
}
