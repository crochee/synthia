//! Glob tool implementation
//!
//! Search for files matching a glob pattern.

use std::path::PathBuf;

use async_trait::async_trait;
use glob::glob;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct GlobRequest {
    pattern: String,
    base_directory: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GlobTool;

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files matching glob patterns."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., **/*.rs)"
                },
                "base_directory": {
                    "type": "string",
                    "description": "Base directory"
                }
            },
            "required": ["pattern"]
        })
    }

    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        true
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: GlobRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let pattern = request.pattern.trim();
        if pattern.is_empty() {
            return CallToolResult::error(vec![Content::text(
                "Pattern cannot be empty".to_string(),
            )]);
        }

        let base_dir = if let Some(ref dir) = request.base_directory {
            let path = PathBuf::from(dir);
            if !path.is_absolute() {
                return CallToolResult::error(vec![Content::text(format!(
                    "base_directory must be an absolute path. Received: '{dir}'"
                ))]);
            }
            if !path.exists() {
                return CallToolResult::error(vec![Content::text(format!(
                    "base_directory does not exist: {}",
                    path.display()
                ))]);
            }
            path
        } else {
            PathBuf::from(".")
        };

        let full_pattern =
            if pattern.starts_with('/') || pattern.starts_with("**") {
                pattern.to_string()
            } else {
                format!("{}/{}", base_dir.display(), pattern)
            };

        let mut matches: Vec<String> = Vec::new();

        let entries = match glob(&full_pattern) {
            Ok(g) => g,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid glob pattern '{pattern}': {e}"
                ))]);
            }
        };

        for path in entries.flatten() {
            matches.push(path.display().to_string());
        }

        matches.sort();

        let output = if matches.is_empty() {
            format!(
                "No files match pattern '{pattern}' in '{}'",
                base_dir.display()
            )
        } else {
            format!(
                "Found {} file(s) matching '{pattern}' in '{}':\n{}",
                matches.len(),
                base_dir.display(),
                matches.join("\n")
            )
        };

        CallToolResult::success(vec![Content::text(output)])
    }
}
