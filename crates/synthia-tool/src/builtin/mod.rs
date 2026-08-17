pub mod read;
pub mod shell;
pub mod todo;
pub mod web;
pub mod write;

use std::{path::PathBuf, sync::Arc};

pub use read::ReadTool;
pub use shell::{DEFAULT_DENY_PATTERNS, ShellSafetyConfig, ShellTool};
pub use todo::TodoWriteTool;
pub use web::WebFetchTool;
pub use write::WriteTool;

use crate::registry::{ToolEntry, ToolRegistry};

/// Build a [`ToolRegistry`] pre-populated with the agent-facing default
/// built-in tool set.
///
/// Currently registers:
/// - `read` — workspace file reader from [`ReadTool`]
/// - `write` — workspace file writer from [`WriteTool`]
/// - `web_fetch` — HTTP fetch from [`WebFetchTool`]
/// - `shell` — bounded shell executor from [`ShellTool`]
/// - `TodoWrite` — structured task list manager from [`TodoWriteTool`]
///
/// `workspace_root` is accepted to preserve the existing caller
/// signature; it is intentionally unused here so the registry can be
/// reused across sessions whose runtime context supplies the actual
/// workspace via `Context`.
pub fn build_default_tool_registry(
    _workspace_root: impl Into<PathBuf>,
) -> ToolRegistry {
    let registry = ToolRegistry::new();

    registry.register_entry(ToolEntry::new(Arc::new(ReadTool::default())));

    registry.register_entry(ToolEntry::new(Arc::new(WriteTool::new())));

    registry.register_entry(ToolEntry::new(Arc::new(WebFetchTool::default())));

    registry.register_entry(ToolEntry::new(Arc::new(ShellTool::new())));

    registry.register_entry(ToolEntry::new(Arc::new(TodoWriteTool::new())));

    registry
}

#[cfg(test)]
mod registry_default_tests {
    use synthia_core::Registry;

    use super::*;

    #[tokio::test]
    async fn registry_has_read_tool() {
        let registry = build_default_tool_registry("/tmp");
        assert!(
            registry.get("read").await.unwrap().is_some(),
            "read tool should be registered"
        );
    }

    #[tokio::test]
    async fn registry_has_write_tool() {
        let registry = build_default_tool_registry("/tmp");
        assert!(
            registry.get("write").await.unwrap().is_some(),
            "write tool should be registered"
        );
    }

    #[tokio::test]
    async fn registry_has_shell_tool() {
        let registry = build_default_tool_registry("/tmp");
        assert!(
            registry.get("shell").await.unwrap().is_some(),
            "shell tool should be registered"
        );
    }

    #[tokio::test]
    async fn registry_has_web_fetch_tool() {
        let registry = build_default_tool_registry("/tmp");
        assert!(
            registry.get("web_fetch").await.unwrap().is_some(),
            "web_fetch tool should be registered"
        );
    }

    #[tokio::test]
    async fn registry_has_todo_write_tool() {
        let registry = build_default_tool_registry("/tmp");
        assert!(
            registry.get("TodoWrite").await.unwrap().is_some(),
            "TodoWrite tool should be registered"
        );
    }

    #[test]
    fn registry_exposes_exactly_five_tools() {
        let registry = build_default_tool_registry("/tmp");
        let snapshot = registry.snapshot();
        assert_eq!(
            snapshot.len(),
            5,
            "default registry should expose exactly read, write, web_fetch, shell, TodoWrite"
        );
        let mut names: Vec<&str> =
            snapshot.iter().map(|m| m.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec!["TodoWrite", "read", "shell", "web_fetch", "write"]
        );
    }
}
