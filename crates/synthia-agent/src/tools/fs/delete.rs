//! DeleteFile tool implementation
//!
//! Delete files from the filesystem.

use std::path::PathBuf;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct DeleteFileRequest {
    file_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeleteTool;

impl Default for DeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DeleteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for DeleteTool {
    fn name(&self) -> &str {
        "deleteFile"
    }

    fn description(&self) -> &str {
        "Delete files. Files must exist."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths to delete"
                }
            },
            "required": ["file_paths"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: DeleteFileRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        if request.file_paths.is_empty() {
            return CallToolResult::error(vec![Content::text(
                "file_paths cannot be empty".to_string(),
            )]);
        }

        let mut deleted: Vec<String> = Vec::new();
        let mut not_found: Vec<String> = Vec::new();
        let mut errors: Vec<(String, String)> = Vec::new();

        for file_path in &request.file_paths {
            let path = PathBuf::from(file_path);

            if !path.is_absolute() {
                errors.push((
                    file_path.clone(),
                    "Path must be absolute. Please provide an absolute path."
                        .to_string(),
                ));
                continue;
            }

            if !path.exists() {
                not_found.push(path.display().to_string());
                continue;
            }

            if !path.is_file() {
                errors.push((
                    path.display().to_string(),
                    "Path is not a file".to_string(),
                ));
                continue;
            }

            match tokio::fs::remove_file(&path).await {
                Ok(()) => deleted.push(path.display().to_string()),
                Err(e) => {
                    errors.push((path.display().to_string(), e.to_string()))
                }
            }
        }

        let mut messages: Vec<String> = Vec::new();

        if !deleted.is_empty() {
            messages.push(format!("Deleted {} file(s):", deleted.len()));
            for path in &deleted {
                messages.push(format!("  ✓ {path}"));
            }
        }

        if !not_found.is_empty() {
            messages
                .push(format!("\nNot found ({} file(s)):", not_found.len()));
            for path in &not_found {
                messages.push(format!("  ✗ {path}"));
            }
        }

        if !errors.is_empty() {
            messages.push(format!("\nErrors ({} file(s)):", errors.len()));
            for (path, error) in &errors {
                messages.push(format!("  ✗ {path}: {error}"));
            }
        }

        CallToolResult::success(vec![Content::text(messages.join("\n"))])
    }
}
