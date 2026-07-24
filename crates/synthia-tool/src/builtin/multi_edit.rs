use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    builtin::path::{check_path_safety, resolve_path},
    traits::Tool,
    types::*,
};

#[derive(Debug, Deserialize)]
struct MultiEditInput {
    path: String,
    edits: Vec<Edit>,
}

#[derive(Debug, Clone, Deserialize)]
struct Edit {
    old_str: String,
    new_str: String,
}

#[derive(Debug, Default)]
pub struct MultiEditTool;

#[async_trait]
impl Tool for MultiEditTool {
    fn name(&self) -> &str {
        "multi_edit"
    }

    fn description(&self) -> &str {
        "Perform multiple find-and-replace operations in a single file. All edits succeed or all are rolled back."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path", "edits"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to edit"
                },
                "edits": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["old_str", "new_str"],
                        "properties": {
                            "old_str": {
                                "type": "string",
                                "description": "Text to find"
                            },
                            "new_str": {
                                "type": "string",
                                "description": "Replacement text"
                            }
                        }
                    },
                    "description": "List of find-and-replace operations"
                }
            }
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let workspace_root = &input.context.workspace_root;
        let edit_input: MultiEditInput =
            match serde_json::from_value(input.input) {
                Ok(v) => v,
                Err(e) => {
                    return ToolOutput::error(format!(
                        "Invalid arguments: {}",
                        e
                    ));
                }
            };

        if edit_input.edits.is_empty() {
            return ToolOutput::error("No edits provided");
        }

        if let Some(err) = check_path_safety(workspace_root, &edit_input.path) {
            return ToolOutput::error(err);
        }
        let resolved = resolve_path(workspace_root, &edit_input.path);

        if !resolved.exists() {
            return ToolOutput::error(format!(
                "File not found: {}",
                resolved.display()
            ));
        }

        let original_content = match std::fs::read_to_string(&resolved) {
            Ok(c) => c,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Failed to read file: {}",
                    e
                ));
            }
        };
        let mut current_content = original_content.clone();
        let mut applied_count = 0;

        for (i, edit) in edit_input.edits.iter().enumerate() {
            if let Some(pos) = current_content.find(&edit.old_str) {
                current_content.replace_range(
                    pos..pos + edit.old_str.len(),
                    &edit.new_str,
                );
                applied_count += 1;
            } else {
                if let Err(e) = std::fs::write(&resolved, &original_content) {
                    return ToolOutput::error(format!(
                        "Failed to restore original file: {}",
                        e
                    ));
                }
                return ToolOutput::error(format!(
                    "Edit {} failed: '{}' not found in file. All changes rolled back.",
                    i + 1,
                    edit.old_str.chars().take(50).collect::<String>()
                ));
            }
        }

        if let Err(e) = std::fs::write(&resolved, &current_content) {
            return ToolOutput::error(format!("Failed to write file: {}", e));
        }

        ToolOutput::text(format!(
            "Applied {} edits to {}",
            applied_count,
            resolved.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn make_input(
        workspace_root: PathBuf,
        input: serde_json::Value,
    ) -> ToolInput {
        ToolInput {
            name: "multi_edit".to_string(),
            input: input.clone(),
            context: ToolExecutionContext::new(
                "s1".to_string(),
                workspace_root,
            ),
        }
    }

    #[tokio::test]
    async fn test_multi_edit_single_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let tool = MultiEditTool;
        let input = make_input(
            dir.path().to_path_buf(),
            serde_json::json!({
                "path": "test.txt",
                "edits": [{"old_str": "world", "new_str": "rust"}]
            }),
        );
        let output = tool.call(input).await;
        assert!(!output.is_error.unwrap_or(false));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello rust");
    }

    #[tokio::test]
    async fn test_multi_edit_multiple_replacements() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "foo bar baz").unwrap();

        let tool = MultiEditTool;
        let input = make_input(
            dir.path().to_path_buf(),
            serde_json::json!({
                "path": "test.txt",
                "edits": [
                    {"old_str": "foo", "new_str": "one"},
                    {"old_str": "bar", "new_str": "two"},
                    {"old_str": "baz", "new_str": "three"}
                ]
            }),
        );
        tool.call(input).await;
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one two three");
    }

    #[tokio::test]
    async fn test_multi_edit_rollback_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        let original = "hello world";
        std::fs::write(&file, original).unwrap();

        let tool = MultiEditTool;
        let input = make_input(
            dir.path().to_path_buf(),
            serde_json::json!({
                "path": "test.txt",
                "edits": [
                    {"old_str": "hello", "new_str": "hi"},
                    {"old_str": "nonexistent", "new_str": "x"}
                ]
            }),
        );
        let output = tool.call(input).await;
        assert!(output.is_error.unwrap_or(false));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    }

    #[tokio::test]
    async fn test_multi_edit_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = MultiEditTool;
        let input = make_input(
            dir.path().to_path_buf(),
            serde_json::json!({
                "path": "nonexistent.txt",
                "edits": [{"old_str": "a", "new_str": "b"}]
            }),
        );
        let output = tool.call(input).await;
        assert!(output.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_multi_edit_empty_edits() {
        let dir = tempfile::tempdir().unwrap();
        let tool = MultiEditTool;
        let input = make_input(
            dir.path().to_path_buf(),
            serde_json::json!({
                "path": "test.txt",
                "edits": []
            }),
        );
        let output = tool.call(input).await;
        assert!(output.is_error.unwrap_or(false));
    }

    #[test]
    fn test_multi_edit_is_not_concurrency_safe() {
        // Multi-edit mutates the same file — concurrent edits would race.
        let tool = MultiEditTool;
        assert!(!tool.is_concurrency_safe());
    }
}
