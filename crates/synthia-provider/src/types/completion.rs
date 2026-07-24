//! The completion request / response types + [`ToolChoice`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    content::Content,
    message::Message,
    models::TokenUsage,
    tool::ToolDefinition,
};
use crate::cache_policy::CachePolicy;

/// `messages` and `tools` are `Arc<Vec<T>>` so that reference equality
/// (`Arc::ptr_eq`) can be used to short-circuit cache policy
/// re-application and to signal that the prompt cache prefix is
/// unchanged between calls. `Arc<Vec<T>>` derefs to `Vec<T>`, so read
/// accesses (`request.tools.iter()`, `request.tools[i]`, etc.) work
/// unchanged and the wire format (JSON array) is identical to
/// `Vec<T>`.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Arc<Vec<Message>>,
    pub tools: Arc<Vec<ToolDefinition>>,
    pub tool_choice: ToolChoice,
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub extra_body:
        Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_policy: Option<CachePolicy>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub enum ToolChoice {
    #[default]
    Auto,
    None,
    Required,
    Specific {
        name: String,
    },
}

#[derive(Clone, Debug)]
pub struct CompletionResponse {
    pub id: String,
    pub model: String,
    pub content: Content,
    pub usage: TokenUsage,
    pub cached: bool,
}
