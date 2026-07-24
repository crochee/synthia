use async_trait::async_trait;
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use crate::{
    builtin::path::{check_path_safety, resolve_path},
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

/// UTF-8 BOM bytes.
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

#[derive(Debug)]
pub struct ReadTool {
    read_history: parking_lot::Mutex<Vec<String>>,
}

impl ReadTool {
    pub fn new() -> Self {
        Self {
            read_history: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn mark_read(&self, path: &str) {
        self.read_history.lock().push(path.to_string());
    }

    pub fn has_read(&self, path: &str) -> bool {
        self.read_history.lock().iter().any(|p| p == path)
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Reads the contents of files"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read."
                },
                "offset": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The line number to start reading from (must be at least 1). Only provide if the file is too large to read at once."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The number of lines to read (must be at least 1, cannot be negative). Only provide if the file is too large to read at once."
                }
            },
            "required": ["file_path"]
        })
    }

    fn is_concurrency_safe(&self) -> bool {
        // Read is pure — same input always produces same output,
        // no shared mutable state (read_history is per-tool-instance).
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
        let file_path = match input
            .input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "file_path is required".to_string())
        {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if let Some(err) = check_path_safety(workspace_root, file_path) {
            return ToolOutput::error(err);
        }
        let resolved = resolve_path(workspace_root, file_path);

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let offset: Option<usize> = input
            .input
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let limit: Option<usize> = input
            .input
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        // Read file in chunks to allow cooperative cancellation
        const CHUNK_SIZE: usize = 64 * 1024;
        let mut file = match tokio::fs::File::open(&resolved).await {
            Ok(f) => f,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Failed to open file '{}': {}",
                    resolved.display(),
                    e
                ));
            }
        };

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let mut all_bytes = Vec::new();
        let mut buffer = vec![0u8; CHUNK_SIZE];
        loop {
            tokio::task::yield_now().await;
            if token.is_cancelled() {
                return ToolOutput::error("operation cancelled");
            }

            let bytes_read = match file.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    return ToolOutput::error(format!(
                        "Failed to read file '{}': {}",
                        resolved.display(),
                        e
                    ));
                }
            };
            all_bytes.extend_from_slice(&buffer[..bytes_read]);
        }

        let bytes = if all_bytes.starts_with(UTF8_BOM) {
            &all_bytes[UTF8_BOM.len()..]
        } else {
            &all_bytes[..]
        };

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let content = match String::from_utf8(bytes.to_vec()) {
            Ok(c) => c,
            Err(e) => {
                return ToolOutput::error(format!(
                    "File '{}' is not valid UTF-8: {}",
                    resolved.display(),
                    e
                ));
            }
        };

        self.mark_read(file_path);

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.unwrap_or(1).saturating_sub(1);
        let end = match (offset, limit) {
            (Some(_), Some(l)) => start + l,
            (Some(_), None) => lines.len(),
            (None, Some(l)) => l,
            (None, None) => lines.len(),
        };

        let selected_lines =
            &lines[start.min(total_lines)..end.min(total_lines)];
        let mut output = String::new();
        for (i, line) in selected_lines.iter().enumerate() {
            let line_num = start + i + 1;
            output.push_str(&format!("{:>4} {}\n", line_num, line));
        }

        if output.is_empty() && selected_lines.is_empty() {
            ToolOutput::text(format!(
                "(file '{}' is empty)",
                resolved.display()
            ))
        } else {
            ToolOutput::text(output)
        }
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let workspace_root = &input.context.workspace_root;
        let file_path = match input
            .input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "file_path is required".to_string())
        {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if let Some(err) = check_path_safety(workspace_root, file_path) {
            return ToolOutput::error(err);
        }
        let resolved = resolve_path(workspace_root, file_path);

        let offset: Option<usize> = input
            .input
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let limit: Option<usize> = input
            .input
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let bytes = match tokio::fs::read(&resolved).await {
            Ok(b) => b,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Failed to read file '{}': {}",
                    resolved.display(),
                    e
                ));
            }
        };

        let bytes = if bytes.starts_with(UTF8_BOM) {
            &bytes[UTF8_BOM.len()..]
        } else {
            &bytes[..]
        };

        let content = match String::from_utf8(bytes.to_vec()) {
            Ok(c) => c,
            Err(e) => {
                return ToolOutput::error(format!(
                    "File '{}' is not valid UTF-8: {}",
                    resolved.display(),
                    e
                ));
            }
        };

        self.mark_read(file_path);

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.unwrap_or(1).saturating_sub(1);
        let end = match (offset, limit) {
            (Some(_), Some(l)) => start + l,
            (Some(_), None) => lines.len(),
            (None, Some(l)) => l,
            (None, None) => lines.len(),
        };

        let selected_lines =
            &lines[start.min(total_lines)..end.min(total_lines)];
        let mut output = String::new();
        for (i, line) in selected_lines.iter().enumerate() {
            let line_num = start + i + 1;
            output.push_str(&format!("{:>4} {}\n", line_num, line));
        }

        if output.is_empty() && selected_lines.is_empty() {
            ToolOutput::text(format!(
                "(file '{}' is empty)",
                resolved.display()
            ))
        } else {
            ToolOutput::text(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::types::ToolExecutionContext;

    fn make_input(file_path: &str) -> ToolInput {
        make_input_with_root(
            std::path::PathBuf::from("/tmp"),
            file_path,
            None,
            None,
        )
    }

    fn make_input_with_root(
        workspace_root: PathBuf,
        file_path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> ToolInput {
        let mut input = serde_json::Map::new();
        input.insert("file_path".to_string(), json!(file_path));
        if let Some(o) = offset {
            input.insert("offset".to_string(), json!(o));
        }
        if let Some(l) = limit {
            input.insert("limit".to_string(), json!(l));
        }
        ToolInput {
            name: "read".to_string(),
            input: serde_json::Value::Object(input),
            context: ToolExecutionContext::new(
                "s1".to_string(),
                workspace_root,
            ),
        }
    }

    #[tokio::test]
    async fn test_read_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hello.txt");
        std::fs::write(&p, "line 1\nline 2\nline 3\n").unwrap();
        let tool = ReadTool::new();
        let out = tool.call(make_input(p.to_str().unwrap())).await;
        let text = out.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("line 1"));
        assert!(text.contains("line 3"));
    }

    #[tokio::test]
    async fn test_read_missing_file() {
        let tool = ReadTool::new();
        let out = tool.call(make_input("/nonexistent/path.txt")).await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_read_line_range() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("lines.txt");
        std::fs::write(&p, "a\nb\nc\nd\ne\n").unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(make_input_with_root(
                dir.path().to_path_buf(),
                "lines.txt",
                Some(2),
                Some(3),
            ))
            .await;
        let text = out.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("   2 b"));
        assert!(text.contains("   3 c"));
        assert!(text.contains("   4 d"));
        assert!(!text.contains("   1 a"));
        assert!(!text.contains("   5 e"));
    }

    #[tokio::test]
    async fn test_read_utf8_bom_stripped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bom.txt");
        let mut bytes = Vec::from(UTF8_BOM);
        bytes.extend_from_slice(b"hello world");
        std::fs::write(&p, &bytes).unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(make_input_with_root(
                dir.path().to_path_buf(),
                "bom.txt",
                None,
                None,
            ))
            .await;
        let text = out.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("hello world"));
        assert!(!text.contains("\u{feff}"));
    }

    #[tokio::test]
    async fn test_read_path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(make_input_with_root(
                dir.path().to_path_buf(),
                "../../../etc/passwd",
                None,
                None,
            ))
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[test]
    fn test_read_is_concurrency_safe() {
        // Read is pure — parallel invocations on different files are safe.
        let tool = ReadTool::new();
        assert!(tool.is_concurrency_safe());
    }
}
