//! Write tool implementation
//!
//! Write content to a file with overwrite/append modes.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct WriteRequest {
    file_path: String,
    content: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    create_directories: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct WriteTool;

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Create or overwrite file. Use Edit for existing files."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "File path"
                },
                "content": {
                    "type": "string",
                    "description": "Content"
                },
                "mode": {
                    "type": "string",
                    "enum": ["overwrite", "append"],
                    "description": "Mode (overwrite/append)",
                    "default": "overwrite"
                },
                "create_directories": {
                    "type": "boolean",
                    "description": "Auto-create parent dirs",
                    "default": true
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: WriteRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let path = std::path::PathBuf::from(&request.file_path);
        if !path.is_absolute() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path must be absolute. Received relative path: '{}'. Please provide an absolute path.",
                request.file_path
            ))]);
        }

        let should_create_dirs = request.create_directories.unwrap_or(true);
        if let Some(parent) = path.parent()
            && !parent.exists()
            && should_create_dirs
            && let Err(e) = tokio::fs::create_dir_all(parent).await
        {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to create parent directory: {e}"
            ))]);
        }

        let mode = request.mode.unwrap_or_else(|| "overwrite".to_string());
        if mode != "overwrite" && mode != "append" {
            return CallToolResult::error(vec![Content::text(format!(
                "Invalid mode: '{mode}'. Mode must be 'overwrite' or 'append'."
            ))]);
        }

        let existed = path.exists();
        let old_text = if existed && mode == "append" {
            Some(tokio::fs::read_to_string(&path).await.unwrap_or_default())
        } else {
            None
        };

        let new_text = match mode.as_str() {
            "append" => {
                format!("{}{}", old_text.unwrap_or_default(), request.content)
            }
            _ => request.content,
        };

        if let Err(e) = tokio::fs::write(&path, &new_text).await {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to write file: {e}"
            ))]);
        }

        let action = if existed { "Updated" } else { "Created" };
        CallToolResult::success(vec![Content::text(format!(
            "{action} file: {}",
            path.display()
        ))])
    }
}
