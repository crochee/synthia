use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{
    builtin::path::{check_path_safety, resolve_path},
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

#[derive(Debug, Deserialize)]
struct GlobArgs {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Default)]
pub struct GlobTool;

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Finds files based on pattern matching"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., '**/*.rs', 'src/**/*.ts')"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search from (defaults to workspace root)"
                }
            }
        })
    }

    fn is_concurrency_safe(&self) -> bool {
        // Pure read-only filesystem query; no shared mutable state.
        true
    }

    async fn call_with_sandbox(
        &self,
        input: ToolInput,
        _sandbox_attempt: &synthia_sandbox::SandboxAttempt,
        token: &CancellationToken,
    ) -> ToolOutput {
        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let workspace_root = &input.context.workspace_root;
        let args: GlobArgs = match serde_json::from_value(input.input.clone()) {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let path_arg = args.path.as_deref().unwrap_or(".");
        if let Some(err) = check_path_safety(workspace_root, path_arg) {
            return ToolOutput::error(err);
        }
        let search_root = resolve_path(workspace_root, path_arg);
        if !search_root.exists() {
            return ToolOutput::text(format!(
                "Search directory not found: {}",
                search_root.display()
            ));
        }

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let full_pattern = if args.pattern.starts_with('/') {
            if let Some(err) = check_path_safety(workspace_root, &args.pattern)
            {
                return ToolOutput::error(err);
            }
            args.pattern.clone()
        } else {
            format!("{}/{}", search_root.display(), args.pattern)
        };

        let matches_result = glob::glob(&full_pattern);
        let glob_iter = match matches_result {
            Ok(iter) => iter,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Invalid glob pattern: {}",
                    e
                ));
            }
        };

        let mut matches: Vec<String> = Vec::new();
        for entry in glob_iter {
            tokio::task::yield_now().await;
            if token.is_cancelled() {
                return ToolOutput::error("operation cancelled");
            }

            match entry {
                Ok(path) => {
                    if path.is_file() {
                        if let Ok(rel) = path.strip_prefix(workspace_root) {
                            matches.push(rel.to_string_lossy().to_string());
                        } else {
                            matches.push(path.to_string_lossy().to_string());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Glob error: {}", e);
                }
            }
        }

        if matches.is_empty() {
            ToolOutput::text("(no matches)".to_string())
        } else {
            matches.sort();
            ToolOutput::text(matches.join("\n"))
        }
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let workspace_root = &input.context.workspace_root;
        let args: GlobArgs = match serde_json::from_value(input.input.clone()) {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        let path_arg = args.path.as_deref().unwrap_or(".");
        if let Some(err) = check_path_safety(workspace_root, path_arg) {
            return ToolOutput::error(err);
        }
        let search_root = resolve_path(workspace_root, path_arg);
        if !search_root.exists() {
            return ToolOutput::text(format!(
                "Search directory not found: {}",
                search_root.display()
            ));
        }

        let full_pattern = if args.pattern.starts_with('/') {
            if let Some(err) = check_path_safety(workspace_root, &args.pattern)
            {
                return ToolOutput::error(err);
            }
            args.pattern.clone()
        } else {
            format!("{}/{}", search_root.display(), args.pattern)
        };

        let matches_result = glob::glob(&full_pattern);
        let glob_iter = match matches_result {
            Ok(iter) => iter,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Invalid glob pattern: {}",
                    e
                ));
            }
        };

        let mut matches: Vec<String> = Vec::new();
        for entry in glob_iter {
            match entry {
                Ok(path) => {
                    if path.is_file() {
                        if let Ok(rel) = path.strip_prefix(workspace_root) {
                            matches.push(rel.to_string_lossy().to_string());
                        } else {
                            matches.push(path.to_string_lossy().to_string());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Glob error: {}", e);
                }
            }
        }

        if matches.is_empty() {
            ToolOutput::text("(no matches)".to_string())
        } else {
            matches.sort();
            ToolOutput::text(matches.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{traits::Tool, types::ToolExecutionContext};

    fn make_input(
        workspace_root: PathBuf,
        args: serde_json::Value,
    ) -> ToolInput {
        ToolInput {
            name: "glob".to_string(),
            input: args,
            context: ToolExecutionContext::new(
                "s1".to_string(),
                workspace_root,
            ),
        }
    }

    #[tokio::test]
    async fn test_glob_find_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();

        let tool = GlobTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"pattern": "**/*.rs"}),
            ))
            .await;
        let text = output.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("test.rs"));
        assert!(text.contains("src/lib.rs"));
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GlobTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"pattern": "**/*.py"}),
            ))
            .await;
        let text = output.content.iter().find_map(|c| c.text()).unwrap();
        assert_eq!(text, "(no matches)");
    }

    #[tokio::test]
    async fn test_glob_path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GlobTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"path": "../../../etc", "pattern": "*.conf"}),
            ))
            .await;
        assert!(output.is_error.unwrap_or(false));
    }

    #[test]
    fn test_glob_is_concurrency_safe() {
        // Read-only filesystem query — parallel invocations are safe.
        let tool = GlobTool;
        assert!(tool.is_concurrency_safe());
    }
}
