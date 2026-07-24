//! File operation tools backed by `synthia-tool` implementations.
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    Tool,
    ToolInput,
    ToolOutput,
    builtin::{ApplyPatchTool, ReadTool, WriteTool},
};
use synthia_tool_orchestrator::{ExecutableTool, adapter::ToolAdapter};

/// `read_file` exposes the [`ReadTool`] behavior under the agent-facing name.
#[derive(Debug, Default)]
pub struct ReadFileTool(ReadTool);

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        self.0.description()
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
                    "description": "The line number to start reading from (must be at least 1)."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The number of lines to read."
                }
            },
            "required": ["file_path"]
        })
    }

    fn is_concurrency_safe(&self) -> bool {
        self.0.is_concurrency_safe()
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        self.0.call(input).await
    }
}

/// `write_file` exposes the [`WriteTool`] behavior under the agent-facing name.
#[derive(Debug, Default)]
pub struct WriteFileTool(WriteTool);

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        self.0.description()
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

    fn requires_permission(&self) -> bool {
        self.0.requires_permission()
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        self.0.call(input).await
    }
}

/// `apply_patch` exposes the [`ApplyPatchTool`] behavior under the agent-facing
/// name.
#[derive(Debug, Default, Clone)]
pub struct ApplyPatchFileTool(ApplyPatchTool);

#[async_trait]
impl Tool for ApplyPatchFileTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        self.0.description()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["patch"],
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "V4A patch text starting with '*** Begin Patch'"
                }
            }
        })
    }

    fn requires_permission(&self) -> bool {
        self.0.requires_permission()
    }

    fn is_concurrency_safe(&self) -> bool {
        self.0.is_concurrency_safe()
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        self.0.call(input).await
    }
}

/// Returns an [`ExecutableTool`] that reads files.
pub fn read_file() -> Arc<dyn ExecutableTool> {
    Arc::new(ToolAdapter::new(Arc::new(ReadFileTool::default())))
}

/// Returns an [`ExecutableTool`] that writes files.
pub fn write_file() -> Arc<dyn ExecutableTool> {
    Arc::new(ToolAdapter::new(Arc::new(WriteFileTool::default())))
}

/// Returns an [`ExecutableTool`] that applies V4A patches.
pub fn apply_patch() -> Arc<dyn ExecutableTool> {
    Arc::new(ToolAdapter::new(Arc::new(ApplyPatchFileTool::default())))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use synthia_tool::{Tool, ToolInput, types::ToolExecutionContext};

    use super::*;

    fn make_input(
        name: &str,
        args: serde_json::Value,
        root: PathBuf,
    ) -> ToolInput {
        ToolInput {
            name: name.to_string(),
            input: args,
            context: ToolExecutionContext::new("s1".to_string(), root),
        }
    }

    #[tokio::test]
    async fn read_file_tool_uses_agent_facing_name() {
        let tool = ReadFileTool::default();
        assert_eq!(tool.name(), "read_file");
        assert!(tool.is_concurrency_safe());
    }

    #[tokio::test]
    async fn write_file_tool_requires_permission() {
        let tool = WriteFileTool::default();
        assert_eq!(tool.name(), "write_file");
        assert!(tool.requires_permission());
    }

    #[tokio::test]
    async fn apply_patch_file_tool_requires_permission_and_serializes() {
        let tool = ApplyPatchFileTool::default();
        assert_eq!(tool.name(), "apply_patch");
        assert!(tool.requires_permission());
        assert!(!tool.is_concurrency_safe());
    }

    #[tokio::test]
    async fn read_file_tool_reads_workspace_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.txt");
        std::fs::write(&p, "hello").unwrap();
        let tool = ReadFileTool::default();
        let input = make_input(
            "read_file",
            serde_json::json!({ "file_path": p.to_str().unwrap() }),
            dir.path().to_path_buf(),
        );
        let out = tool.call(input).await;
        let text = out.content.iter().find_map(|c| c.text()).unwrap();
        assert!(text.contains("hello"));
    }

    #[tokio::test]
    async fn write_file_tool_writes_workspace_file() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::default();
        let input = make_input(
            "write_file",
            serde_json::json!({ "path": "out.txt", "content": "data" }),
            dir.path().to_path_buf(),
        );
        let out = tool.call(input).await;
        assert!(!out.is_error.unwrap_or(false));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("out.txt")).unwrap(),
            "data"
        );
    }

    #[tokio::test]
    async fn apply_patch_file_tool_applies_patch() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("patch.txt");
        std::fs::write(&p, "old\n").unwrap();
        let tool = ApplyPatchFileTool::default();
        let input = make_input(
            "apply_patch",
            serde_json::json!({
                "patch": format!(
                    "*** Begin Patch\n*** Update File: {}\n@@\n-old\n+new\n*** End Patch\n",
                    p.file_name().unwrap().to_str().unwrap()
                )
            }),
            dir.path().to_path_buf(),
        );
        let out = tool.call(input).await;
        assert!(!out.is_error.unwrap_or(false));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new\n");
    }
}
