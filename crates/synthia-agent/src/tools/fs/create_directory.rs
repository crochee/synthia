//! Create directory tool implementation
//!
//! Create a new directory or ensure a directory exists.

use std::path::PathBuf;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct CreateDirectoryRequest {
    path: String,
}

#[derive(Debug, Clone)]
pub struct CreateDirectoryTool;

impl Default for CreateDirectoryTool {
    fn default() -> Self {
        Self::new()
    }
}

impl CreateDirectoryTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for CreateDirectoryTool {
    fn name(&self) -> &str {
        "createDirectory"
    }

    fn description(&self) -> &str {
        "Create directory (including parents). Succeeds if exists."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path"
                }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: CreateDirectoryRequest = match serde_json::from_value(args)
        {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let path = PathBuf::from(&request.path);
        if !path.is_absolute() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path must be absolute. Received relative path: '{}'. Please provide an absolute path.",
                request.path
            ))]);
        }

        let existed = path.exists();

        if let Err(e) = tokio::fs::create_dir_all(&path).await {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to create directory: {e}"
            ))]);
        }

        let message = if existed {
            format!("Directory already exists: {}", path.display())
        } else {
            format!("Successfully created directory: {}", path.display())
        };

        CallToolResult::success(vec![Content::text(message)])
    }
}
