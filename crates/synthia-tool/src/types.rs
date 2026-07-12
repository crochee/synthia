use std::path::PathBuf;

use serde::{Deserialize, Serialize};
pub use synthia_core::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchMode {
    Fork,
    Teammate,
    Worktree,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    pub session_id: String,
    pub workspace_root: PathBuf,
    pub caller_agent: String,
    pub dispatch_mode: DispatchMode,
    /// Conversation messages visible to the current turn. Populated by
    /// the agent runtime so that context-aware tools (e.g.
    /// `self_reflect`) can review the session history without requiring
    /// the LLM to pass it as arguments.
    pub messages: Vec<synthia_provider::types::Message>,
}

impl ToolExecutionContext {
    pub fn new(session_id: String, workspace_root: PathBuf) -> Self {
        Self {
            session_id,
            workspace_root,
            caller_agent: "default".to_string(),
            dispatch_mode: DispatchMode::Fork,
            messages: Vec::new(),
        }
    }

    /// Attach the conversation messages that should be visible to the
    /// tool execution.
    pub fn with_messages(
        mut self,
        messages: Vec<synthia_provider::types::Message>,
    ) -> Self {
        self.messages = messages;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ToolInput {
    pub name: String,
    pub input: serde_json::Value,
    pub context: ToolExecutionContext,
}

/// Reason a [`ToolOutput`] was truncated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TruncatedBy {
    /// Truncated to a maximum number of output lines.
    Lines { shown: usize, total: usize },
    /// Truncated to a maximum number of output bytes.
    Bytes { shown: usize, total: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: Vec<synthia_provider::types::ContentPart>,
    pub is_error: Option<bool>,
    /// Structured metadata accompanying the tool result (e.g. counts,
    /// timing, truncation reason). Defaults to empty for backward
    /// compatibility.
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
    /// Optional truncation reason — populated when the orchestrator or
    /// the tool itself trimmed the output before returning it to the
    /// LLM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated_by: Option<TruncatedBy>,
}

impl ToolOutput {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: vec![synthia_provider::types::ContentPart::Text(
                synthia_provider::types::TextContent {
                    text: content.into(),
                    cache_control: None,
                },
            )],
            is_error: None,
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: vec![synthia_provider::types::ContentPart::Text(
                synthia_provider::types::TextContent {
                    text: content.into(),
                    cache_control: None,
                },
            )],
            is_error: Some(true),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }

    pub fn is_text(&self) -> bool {
        self.is_error.is_none()
    }

    /// Build a [`ToolOutput`] from a raw [`serde_json::Value`], using the
    /// JSON string form as the textual content. This is the default
    /// constructor used by the new `Tool::output` adapter.
    pub fn from_raw(raw: serde_json::Value) -> Self {
        Self::text(raw.to_string())
    }

    /// Attach a truncation reason. Builder-style; returns the modified
    /// output for chaining.
    pub fn with_truncated_by(mut self, truncated_by: TruncatedBy) -> Self {
        self.truncated_by = Some(truncated_by);
        self
    }

    /// Insert a metadata entry. Builder-style; returns the modified
    /// output for chaining.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

impl From<String> for ToolOutput {
    fn from(content: String) -> Self {
        Self::text(content)
    }
}
