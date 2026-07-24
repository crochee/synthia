use std::{
    fs,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{
    builtin::path::{check_path_safety, resolve_path},
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

#[derive(Debug, Deserialize)]
struct GrepArgs {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default)]
    context_lines: Option<usize>,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(Debug, Default)]
pub struct GrepTool;

impl GrepTool {
    fn is_binary(path: &Path) -> bool {
        if let Ok(bytes) = fs::read(path) {
            bytes.iter().take(8192).any(|&b| b == 0)
        } else {
            false
        }
    }

    fn search_file(
        file_path: &Path,
        regex: &Regex,
        context_lines: usize,
    ) -> Vec<String> {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let lines: Vec<&str> = content.lines().collect();
        let mut results = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                let start = i.saturating_sub(context_lines);
                let end = (i + context_lines + 1).min(lines.len());

                for (j, line_j) in
                    lines.iter().enumerate().take(end).skip(start)
                {
                    let prefix = if j == i { ">" } else { " " };
                    results.push(format!(
                        "{}:{}:{} {}",
                        file_path.display(),
                        j + 1,
                        prefix,
                        line_j
                    ));
                }
            }
        }

        results
    }

    fn collect_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if root.is_file() {
            if !Self::is_binary(root) {
                files.push(root.to_path_buf());
            }
            return files;
        }
        if root.is_dir()
            && let Ok(entries) = fs::read_dir(root)
        {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                if file_name == ".git"
                    || file_name == "node_modules"
                    || file_name == "target"
                {
                    continue;
                }
                if path.is_file() {
                    if !Self::is_binary(&path) {
                        files.push(path);
                    }
                } else if path.is_dir() {
                    files.extend(Self::collect_files(&path));
                }
            }
        }
        files
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Searches for patterns in file contents"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to workspace root)"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines before and after match"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return"
                }
            }
        })
    }

    fn is_concurrency_safe(&self) -> bool {
        // Pure read-only filesystem search; no shared mutable state.
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
        let args: GrepArgs = match serde_json::from_value(input.input.clone()) {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let re = if args.case_insensitive {
            Regex::new(&format!("(?i){}", args.pattern))
        } else {
            Regex::new(&args.pattern)
        };
        let re = match re {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!("Invalid regex: {}", e));
            }
        };

        let path_arg = args.path.as_deref().unwrap_or(".");
        if let Some(err) = check_path_safety(workspace_root, path_arg) {
            return ToolOutput::error(err);
        }
        let search_path = resolve_path(workspace_root, path_arg);
        let context = args.context_lines.unwrap_or(0);
        let max_results = args.max_results.unwrap_or(1000);

        let files = Self::collect_files(&search_path);
        let mut all_results = Vec::new();

        for file in files {
            tokio::task::yield_now().await;
            if token.is_cancelled() {
                return ToolOutput::error("operation cancelled");
            }

            let results = Self::search_file(&file, &re, context);
            all_results.extend(results);
            if all_results.len() >= max_results {
                break;
            }
        }

        if all_results.is_empty() {
            ToolOutput::text("(no matches)".to_string())
        } else {
            let truncated = if all_results.len() > max_results {
                all_results.truncate(max_results);
                format!(
                    "\n... (results truncated, max {} reached)",
                    max_results
                )
            } else {
                String::new()
            };
            ToolOutput::text(format!("{}{}", all_results.join("\n"), truncated))
        }
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let workspace_root = &input.context.workspace_root;
        let args: GrepArgs = match serde_json::from_value(input.input.clone()) {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        let re = if args.case_insensitive {
            Regex::new(&format!("(?i){}", args.pattern))
        } else {
            Regex::new(&args.pattern)
        };
        let re = match re {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!("Invalid regex: {}", e));
            }
        };

        let path_arg = args.path.as_deref().unwrap_or(".");
        if let Some(err) = check_path_safety(workspace_root, path_arg) {
            return ToolOutput::error(err);
        }
        let search_path = resolve_path(workspace_root, path_arg);
        let context = args.context_lines.unwrap_or(0);
        let max_results = args.max_results.unwrap_or(1000);

        let files = Self::collect_files(&search_path);
        let mut all_results = Vec::new();

        for file in files {
            let results = Self::search_file(&file, &re, context);
            all_results.extend(results);
            if all_results.len() >= max_results {
                break;
            }
        }

        if all_results.is_empty() {
            ToolOutput::text("(no matches)".to_string())
        } else {
            let truncated = if all_results.len() > max_results {
                all_results.truncate(max_results);
                format!(
                    "\n... (results truncated, max {} reached)",
                    max_results
                )
            } else {
                String::new()
            };
            ToolOutput::text(format!("{}{}", all_results.join("\n"), truncated))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{traits::Tool, types::ToolExecutionContext};

    fn make_input(
        workspace_root: PathBuf,
        args: serde_json::Value,
    ) -> ToolInput {
        ToolInput {
            name: "grep".to_string(),
            input: args,
            context: ToolExecutionContext::new(
                "s1".to_string(),
                workspace_root,
            ),
        }
    }

    #[tokio::test]
    async fn test_grep_find_pattern() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("test.rs"),
            "fn main() {\n    println!(\"hello\");\n}",
        )
        .unwrap();

        let tool = GrepTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"pattern": "fn main"}),
            ))
            .await;
        let text = output.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("fn main"));
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();

        let tool = GrepTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"pattern": "nonexistent"}),
            ))
            .await;
        let text = output.content.iter().find_map(|c| c.text()).unwrap();
        assert_eq!(text, "(no matches)");
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "Hello World").unwrap();

        let tool = GrepTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"pattern": "hello", "case_insensitive": true}),
            ))
            .await;
        let text = output.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("Hello World"));
    }

    #[tokio::test]
    async fn test_grep_skips_binary_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("binary.bin"),
            [0u8, 1, 2, 3, b'h', b'e', b'l', b'l', b'o'],
        )
        .unwrap();

        let tool = GrepTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"pattern": "hello"}),
            ))
            .await;
        let text = output.content.iter().find_map(|c| c.text()).unwrap();
        assert_eq!(text, "(no matches)");
    }

    #[tokio::test]
    async fn test_grep_path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GrepTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"path": "../../../etc", "pattern": "root"}),
            ))
            .await;
        assert!(output.is_error.unwrap_or(false));
    }

    #[test]
    fn test_grep_is_concurrency_safe() {
        // Read-only filesystem search — parallel invocations are safe.
        let tool = GrepTool;
        assert!(tool.is_concurrency_safe());
    }
}
