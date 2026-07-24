//! The 3 raw Anthropic SSE event structs. Used by both the
//! legacy [`super::legacy::StreamProcessor`] and the new
//! [`super::v2::StreamProcessorV2`].

use serde::Deserialize;

/// Top-level Anthropic SSE event (1 per SSE message). Only
/// the fields needed by either stream processor are modeled.
#[derive(Debug, Deserialize)]
pub struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub content_block: Option<AnthropicStreamContentBlock>,
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub delta: Option<AnthropicStreamDelta>,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// Content-block descriptor carried by `content_block_start` events.
#[derive(Debug, Deserialize)]
pub struct AnthropicStreamContentBlock {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
}

/// Delta descriptor carried by `content_block_delta` events.
#[derive(Debug, Deserialize)]
pub struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub partial_json: Option<String>,
    /// Anthropic `signature_delta` value emitted alongside the final
    /// `thinking_delta` for the same content block. Used to preserve
    /// reasoning continuity across turns.
    #[serde(default)]
    pub signature: Option<String>,
}
