//! ToolInput, ToolOutput, ToolError, ToolContext, ToolMetadata.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::tool::capability::ToolCapabilities;

/// Input to a tool invocation.
#[derive(Debug, Clone)]
pub struct ToolInput {
    /// Raw JSON input from LLM.
    pub raw: serde_json::Value,
    /// Tool name.
    pub name: String,
    /// Session ID.
    pub session_id: String,
    /// Workspace root.
    pub workspace_root: PathBuf,
}

/// Output from a tool invocation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Content parts (text, images, etc).
    #[serde(default)]
    pub content: Vec<ContentPart>,
    /// Structured output data.
    #[serde(default)]
    pub structured: Option<serde_json::Value>,
    /// Execution metadata.
    #[serde(default)]
    pub metadata: ToolMetadata,
    /// Whether this is an error output.
    #[serde(default)]
    pub is_error: bool,
}

/// Content part (text or image).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentPart {
    Text { text: String },
    Image { url: String, mime_type: String },
}

impl ToolOutput {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: vec![ContentPart::Text {
                text: content.into(),
            }],
            structured: None,
            metadata: ToolMetadata::default(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: vec![ContentPart::Text {
                text: content.into(),
            }],
            structured: None,
            metadata: ToolMetadata::default(),
            is_error: true,
        }
    }
}

/// Tool execution metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Execution duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<Duration>,
    /// Input token count (approximate).
    #[serde(default)]
    pub tokens_in: u32,
    /// Output token count (approximate).
    #[serde(default)]
    pub tokens_out: u32,
    /// Whether output was truncated.
    #[serde(default)]
    pub truncated: bool,
    /// Managed file paths for spilled output.
    #[serde(default)]
    pub managed_paths: Vec<PathBuf>,
}

/// Tool error with typed variants.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("capability denied: need {need} for service {service}")]
    CapabilityDenied { service: String, need: &'static str },
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("stale tool: {name} generation {seen} != {current}")]
    Stale {
        name: String,
        seen: u64,
        current: u64,
    },
    #[error("cancelled")]
    Cancelled,
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

/// Context provided to tool execution.
/// Carries CapabilityBroker (NOT full ServiceRegistry) per security B5.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Per-tool capability allowlist.
    pub capabilities: ToolCapabilities,
    /// Session ID.
    pub session_id: String,
    /// Workspace root.
    pub workspace_root: PathBuf,
    /// Caller agent ID.
    pub caller_agent: String,
}
