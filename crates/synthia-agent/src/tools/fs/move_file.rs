//! Move file tool implementation
//!
//! Move or rename files and directories.

use std::path::PathBuf;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct MoveFileRequest {
    source: String,
    destination: String,
}

#[derive(Debug, Clone)]
pub struct MoveFileTool;

impl Default for MoveFileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MoveFileTool {
    fn name(&self) -> &str {
        "moveFile"
    }

    fn description(&self) -> &str {
        "Move/rename files or dirs. Fails if destination exists."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Source path"
                },
                "destination": {
                    "type": "string",
                    "description": "Destination path"
                }
            },
            "required": ["source", "destination"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: MoveFileRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let source = PathBuf::from(&request.source);
        if !source.is_absolute() {
            return CallToolResult::error(vec![Content::text(format!(
                "Source path must be absolute. Received relative path: '{}'. Please provide an absolute path.",
                request.source
            ))]);
        }

        let destination = PathBuf::from(&request.destination);
        if !destination.is_absolute() {
            return CallToolResult::error(vec![Content::text(format!(
                "Destination path must be absolute. Received relative path: '{}'. Please provide an absolute path.",
                request.destination
            ))]);
        }

        if !source.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "Source does not exist: {}",
                source.display()
            ))]);
        }

        if destination.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "Destination already exists: {}",
                destination.display()
            ))]);
        }

        if let Some(parent) = destination.parent()
            && !parent.exists()
            && let Err(e) = tokio::fs::create_dir_all(parent).await
        {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to create parent directory: {e}"
            ))]);
        }

        let is_dir = source.is_dir();

        if let Err(e) = tokio::fs::rename(&source, &destination).await {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to move: {e}"
            ))]);
        }

        let action = if is_dir { "directory" } else { "file" };
        let message = format!(
            "Successfully moved {} from '{}' to '{}'",
            action,
            source.display(),
            destination.display()
        );

        CallToolResult::success(vec![Content::text(message)])
    }
}
