//! Edit tool implementation
//!
//! Make exact string replacements in files with batch editing support.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct EditOperation {
    #[serde(alias = "old", alias = "old_string")]
    old_string: String,
    #[serde(alias = "new", alias = "new_string")]
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct EditRequest {
    file_path: String,
    #[serde(default)]
    edit: Option<EditOperation>,
    #[serde(default)]
    edits: Option<Vec<EditOperation>>,
    #[serde(default)]
    diff: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct EditTool;

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EditTool {
    pub fn new() -> Self {
        Self
    }

    fn apply_edit(content: &str, edit: &EditOperation) -> (String, usize) {
        let count = if edit.replace_all {
            content.matches(&edit.old_string).count()
        } else if content.contains(&edit.old_string) {
            1
        } else {
            0
        };

        let new_content = if edit.replace_all {
            content.replace(&edit.old_string, &edit.new_string)
        } else {
            content.replacen(&edit.old_string, &edit.new_string, 1)
        };

        (new_content, count)
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Replace exact text in file. Supports batch edits."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "File path"
                },
                "edit": {
                    "type": "object",
                    "description": "Edit operation",
                    "properties": {
                        "old_string": {"type": "string", "description": "String to replace"},
                        "new_string": {"type": "string", "description": "Replacement"},
                        "replace_all": {"type": "boolean", "default": false, "description": "Replace all"}
                    },
                    "required": ["old_string", "new_string"]
                },
                "edits": {
                    "type": "array",
                    "description": "Batch operations",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": {"type": "string", "description": "String to replace"},
                            "new_string": {"type": "string", "description": "Replacement"},
                            "replace_all": {"type": "boolean", "default": false}
                        },
                        "required": ["old_string", "new_string"]
                    }
                },
                "diff": {
                    "type": "boolean",
                    "default": false,
                    "description": "Return diff"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: EditRequest = match serde_json::from_value(args) {
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

        if !path.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "File does not exist: {}",
                path.display()
            ))]);
        }

        if !path.is_file() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path is not a file: {}",
                path.display()
            ))]);
        }

        let edits: Vec<EditOperation> = if let Some(edit) = request.edit {
            vec![edit]
        } else if let Some(edits) = request.edits {
            if edits.is_empty() {
                return CallToolResult::error(vec![Content::text(
                    "No edits provided".to_string(),
                )]);
            }
            edits
        } else {
            return CallToolResult::error(vec![Content::text(
                "Either 'edit' or 'edits' must be provided".to_string(),
            )]);
        };

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file: {e}"
                ))]);
            }
        };

        let original_content = content.clone();
        let mut current_content = content;
        let mut total_replacements = 0;
        let mut failed_edits = Vec::new();

        for edit in &edits {
            if edit.old_string == edit.new_string {
                failed_edits.push(
                    "Edit skipped: old_string and new_string are identical"
                        .to_string(),
                );
                continue;
            }

            let (new_content, count) = Self::apply_edit(&current_content, edit);
            if count == 0 {
                failed_edits.push(format!(
                    "Edit failed: old_string '{}' not found in file",
                    truncate_string(&edit.old_string, 50)
                ));
            } else {
                total_replacements += count;
                current_content = new_content;
            }
        }

        if current_content == original_content {
            return CallToolResult::error(vec![Content::text(
                "No replacements were made. The old string was not found in the file.".to_string(),
            )]);
        }

        if let Err(e) = tokio::fs::write(&path, &current_content).await {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to write file: {e}"
            ))]);
        }

        let mut output_parts = Vec::new();
        output_parts.push(format!(
            "Successfully applied {} edit(s) with {} total replacement(s) in {}",
            edits.len(),
            total_replacements,
            path.display()
        ));

        if request.diff.unwrap_or(false) {
            let diff_output = generate_unified_diff(
                &original_content,
                &current_content,
                &path,
            );
            output_parts.push("\n--- Diff ---".to_string());
            output_parts.push(diff_output);
        }

        if !failed_edits.is_empty() {
            output_parts.push("\nWarnings:".to_string());
            for msg in failed_edits {
                output_parts.push(format!("- {msg}"));
            }
        }

        CallToolResult::success(vec![Content::text(output_parts.join("\n"))])
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

fn generate_unified_diff(
    old_content: &str,
    new_content: &str,
    _path: &std::path::Path,
) -> String {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let mut result = String::new();
    for line in &old_lines {
        result.push('-');
        result.push_str(line);
        result.push('\n');
    }
    result.push_str("---\n");
    for line in &new_lines {
        result.push('+');
        result.push_str(line);
        result.push('\n');
    }

    if old_lines == new_lines {
        "(no visible changes — possibly whitespace-only)".to_string()
    } else {
        result.trim_end().to_string()
    }
}
