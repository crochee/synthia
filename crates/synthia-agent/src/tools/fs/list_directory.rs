//! List directory tool implementation
//!
//! List files and directories in a given path.

use std::path::PathBuf;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::{AgentError, tools::Tool};

#[derive(Debug, Clone, Deserialize)]
struct ListDirectoryRequest {
    path: String,
}

#[derive(Debug, Clone)]
pub struct ListDirectoryTool;

impl Default for ListDirectoryTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ListDirectoryTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "ListDirectory"
    }

    fn description(&self) -> &str {
        "List files and dirs at path. Prefixed with [FILE]/[DIR]. Sorted alphabetically."
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
        let request: ListDirectoryRequest = match serde_json::from_value(args) {
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

        if !path.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "Directory does not exist: {}",
                path.display()
            ))]);
        }

        if !path.is_dir() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path is not a directory: {}",
                path.display()
            ))]);
        }

        let mut entries = match tokio::fs::read_dir(&path).await {
            Ok(e) => e,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to read directory: {e}"
                ))]);
            }
        };

        let mut result: Vec<String> = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| {
                AgentError::InvalidOperation(format!(
                    "Failed to read entry: {e}"
                ))
            })
            .ok()
            .flatten()
        {
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!("Failed to get file type: {e}"),
                    )]);
                }
            };

            let name = entry.file_name().to_string_lossy().to_string();
            let prefix = if file_type.is_dir() {
                "[DIR]"
            } else {
                "[FILE]"
            };
            result.push(format!("{prefix} {name}"));
        }

        result.sort();

        let output = if result.is_empty() {
            format!("Directory '{}' is empty.", path.display())
        } else {
            format!("Contents of '{}':\n{}", path.display(), result.join("\n"))
        };

        CallToolResult::success(vec![Content::text(output)])
    }
}
