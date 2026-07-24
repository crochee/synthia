//! Search tools backed by `synthia-tool` implementations.
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    Tool,
    ToolInput,
    ToolOutput,
    builtin::{GlobTool, GrepTool},
};
use synthia_tool_orchestrator::{ExecutableTool, adapter::ToolAdapter};

/// Combined search tool that dispatches to [`GlobTool`] or [`GrepTool`]
/// depending on the provided arguments.
#[derive(Debug, Default)]
pub struct SearchFilesTool;

#[async_trait]
impl Tool for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for files by glob pattern or for patterns in file contents"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to find files (e.g., '**/*.rs')"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (defaults to workspace root)"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case insensitive content search"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines before and after a content match"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of content search results to return"
                }
            }
        })
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let has_pattern = input.input.get("pattern").is_some();
        let has_glob = input.input.get("glob").is_some();

        if has_pattern {
            GrepTool
                .call(ToolInput {
                    name: "grep".to_string(),
                    input: input.input,
                    context: input.context,
                })
                .await
        } else if has_glob {
            let mut args = input.input.clone();
            if let Some(glob) = args.get("glob").cloned() {
                args["pattern"] = glob;
            }
            GlobTool
                .call(ToolInput {
                    name: "glob".to_string(),
                    input: args,
                    context: input.context,
                })
                .await
        } else {
            ToolOutput::error(
                "search_files requires either 'pattern' (content search) or 'glob' (file search)",
            )
        }
    }
}

/// Returns an [`ExecutableTool`] that searches files by glob or content.
pub fn search_files() -> Arc<dyn ExecutableTool> {
    Arc::new(ToolAdapter::new(Arc::new(SearchFilesTool)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use synthia_tool::{Tool, ToolInput, types::ToolExecutionContext};

    use super::*;

    fn make_input(args: serde_json::Value, root: PathBuf) -> ToolInput {
        ToolInput {
            name: "search_files".to_string(),
            input: args,
            context: ToolExecutionContext::new("s1".to_string(), root),
        }
    }

    #[tokio::test]
    async fn search_files_tool_searches_by_glob() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        let tool = SearchFilesTool;
        let input = make_input(
            serde_json::json!({ "glob": "**/*.rs" }),
            dir.path().to_path_buf(),
        );
        let out = tool.call(input).await;
        let text = out.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("a.rs"));
        assert!(!text.contains("b.txt"));
    }

    #[tokio::test]
    async fn search_files_tool_searches_by_pattern() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("src.rs"), "fn main() {}").unwrap();
        let tool = SearchFilesTool;
        let input = make_input(
            serde_json::json!({ "pattern": "fn main" }),
            dir.path().to_path_buf(),
        );
        let out = tool.call(input).await;
        let text = out.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("fn main"));
    }

    #[tokio::test]
    async fn search_files_tool_requires_glob_or_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SearchFilesTool;
        let input = make_input(serde_json::json!({}), dir.path().to_path_buf());
        let out = tool.call(input).await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[test]
    fn search_files_tool_is_concurrency_safe() {
        let tool = SearchFilesTool;
        assert!(tool.is_concurrency_safe());
    }
}
