use std::io::Write as _;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::{
    builtin::path::{check_path_safety, resolve_path},
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

#[derive(Debug, Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
    #[serde(default)]
    append: bool,
}

#[derive(Debug, Default)]
pub struct WriteTool;

fn atomic_write_file(
    resolved: &std::path::Path,
    content: &str,
) -> std::io::Result<()> {
    let parent = resolved.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;

    let tmp_name = format!(".{}.tmp", uuid::Uuid::new_v4());
    let tmp_path = parent.join(tmp_name);

    let mut file = std::fs::File::create(&tmp_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);

    std::fs::rename(&tmp_path, resolved)?;
    Ok(())
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Creates or overwrites files"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to write to"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                },
                "append": {
                    "type": "boolean",
                    "description": "If true, append to file instead of overwriting"
                }
            }
        })
    }

    fn execution_mode(&self) -> crate::traits::ExecutionMode {
        // Write mutates the filesystem; never run two copies in
        // parallel against the same path.
        crate::traits::ExecutionMode::Sequential
    }

    fn requires_permission(&self) -> bool {
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
        let args: WriteArgs = match serde_json::from_value(input.input.clone())
        {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        if let Some(err) = check_path_safety(workspace_root, &args.path) {
            return ToolOutput::error(err);
        }
        let resolved = resolve_path(workspace_root, &args.path);

        if args.append && resolved.exists() {
            let mut file = match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&resolved)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    return ToolOutput::error(format!(
                        "Failed to open file for append: {}",
                        e
                    ));
                }
            };

            if token.is_cancelled() {
                return ToolOutput::error("operation cancelled");
            }

            if let Err(e) = file.write_all(args.content.as_bytes()).await {
                return ToolOutput::error(format!(
                    "Failed to write file: {}",
                    e
                ));
            }
        } else if let Err(e) = atomic_write_file(&resolved, &args.content) {
            return ToolOutput::error(format!(
                "Failed to write file '{}': {}",
                resolved.display(),
                e
            ));
        }

        if token.is_cancelled() {
            return ToolOutput::error("operation cancelled");
        }

        ToolOutput::text(format!(
            "Written {} bytes to {}",
            args.content.len(),
            resolved.display()
        ))
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let workspace_root = &input.context.workspace_root;
        let args: WriteArgs = match serde_json::from_value(input.input.clone())
        {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        if let Some(err) = check_path_safety(workspace_root, &args.path) {
            return ToolOutput::error(err);
        }
        let resolved = resolve_path(workspace_root, &args.path);

        if args.append && resolved.exists() {
            let mut file = match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&resolved)
            {
                Ok(f) => f,
                Err(e) => {
                    return ToolOutput::error(format!(
                        "Failed to open file for append: {}",
                        e
                    ));
                }
            };
            if let Err(e) = file.write_all(args.content.as_bytes()) {
                return ToolOutput::error(format!(
                    "Failed to write file: {}",
                    e
                ));
            }
        } else if let Err(e) = atomic_write_file(&resolved, &args.content) {
            return ToolOutput::error(format!(
                "Failed to write file '{}': {}",
                resolved.display(),
                e
            ));
        }

        ToolOutput::text(format!(
            "Written {} bytes to {}",
            args.content.len(),
            resolved.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::types::ToolExecutionContext;

    fn make_input(
        workspace_root: PathBuf,
        args: serde_json::Value,
    ) -> ToolInput {
        ToolInput {
            name: "write".to_string(),
            input: args,
            context: ToolExecutionContext::new(
                "s1".to_string(),
                workspace_root,
            ),
        }
    }

    #[tokio::test]
    async fn test_write_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool;
        let output = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"path": "test.txt", "content": "hello"}),
            ))
            .await;
        assert!(
            output.content.iter().any(|c| c
                .text()
                .map(|t| t.contains("Written"))
                .unwrap_or(false))
        );
        assert!(dir.path().join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_write_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool;
        std::fs::write(dir.path().join("test.txt"), "old").unwrap();

        tool.call(make_input(
            dir.path().to_path_buf(),
            json!({"path": "test.txt", "content": "new"}),
        ))
        .await;
        assert_eq!(
            std::fs::read_to_string(dir.path().join("test.txt")).unwrap(),
            "new"
        );
    }

    #[tokio::test]
    async fn test_write_append() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool;
        std::fs::write(dir.path().join("test.txt"), "hello").unwrap();

        tool.call(make_input(
            dir.path().to_path_buf(),
            json!({"path": "test.txt", "content": " world", "append": true}),
        ))
        .await;
        assert_eq!(
            std::fs::read_to_string(dir.path().join("test.txt")).unwrap(),
            "hello world"
        );
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool;

        tool.call(make_input(
            dir.path().to_path_buf(),
            json!({"path": "a/b/c/test.txt", "content": "data"}),
        ))
        .await;
        assert!(dir.path().join("a/b/c/test.txt").exists());
    }

    #[tokio::test]
    async fn test_write_path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool;

        let result = tool
            .call(make_input(
                dir.path().to_path_buf(),
                json!({"path": "../../../etc/passwd", "content": "x"}),
            ))
            .await;
        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn test_write_is_not_concurrency_safe() {
        // Write mutates filesystem — must run serially to avoid race conditions.
        let tool = WriteTool;
        assert!(!tool.is_concurrency_safe());
    }
}
