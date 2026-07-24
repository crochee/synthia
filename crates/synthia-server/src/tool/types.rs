//! Tool types for API requests and responses

use serde::{Deserialize, Serialize};
use serde_json::Value;
use synthia_provider::ToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_safe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecuteRequest {
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecuteResponse {
    pub success: bool,
    pub result: ToolResult,
}

pub fn tool_info_from_tool(
    tool: std::sync::Arc<dyn synthia_agent::tools::Tool>,
) -> ToolInfo {
    let params = tool.parameters();
    ToolInfo {
        name: tool.name().to_string(),
        description: tool.description().to_string(),
        parameters: params.clone(),
        annotations: Some(ToolAnnotations {
            read_only: Some(tool.is_read_only(&params)),
            destructive: Some(tool.is_mutating(&params)),
            concurrency_safe: Some(tool.is_concurrency_safe(&params)),
            tool_kind: Some(format!("{:?}", tool.tool_kind())),
        }),
    }
}
