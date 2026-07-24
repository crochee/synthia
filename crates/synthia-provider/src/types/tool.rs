//! The tool-related wire types: [`ToolUse`] (LLM → agent),
//! [`ToolResult`] (agent → LLM), [`ToolDefinition`] (registered
//! tool manifest), and [`ResourceLink`] (MCP-style resource
//! reference).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use synthia_cache_mark::CacheControlMark;

use super::content::{ContentPart, TextContent};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: Vec<ContentPart>,
    pub structured_content: Option<Value>,
    pub is_error: Option<bool>,
}

impl ToolResult {
    pub fn new(
        tool_use_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: vec![ContentPart::Text(TextContent {
                text: text.into(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: None,
        }
    }

    pub fn error(
        tool_use_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: vec![ContentPart::Text(TextContent {
                text: text.into(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: Some(true),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_control: Option<CacheControlMark>,
}

impl ToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            cache_control: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLink {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}
