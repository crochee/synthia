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

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: Vec<synthia_provider::types::ContentPart>,
    pub is_error: Option<bool>,
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
        }
    }

    pub fn is_text(&self) -> bool {
        self.is_error.is_none()
    }
}

impl From<String> for ToolOutput {
    fn from(content: String) -> Self {
        Self::text(content)
    }
}
