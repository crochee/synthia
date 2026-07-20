//! Default tool registry construction.
//!
//! Provides a single entry point for building a [`ToolRegistry`] that
//! exposes exactly the built-in tools the agent runtime can execute.

use std::{path::PathBuf, sync::Arc};

use synthia_tool::registry::ToolRegistry;
use synthia_tool_bash::{BashTool, CommandBlacklist, CommandManager};

use crate::tools::builtins::{
    file_tools::{ApplyPatchFileTool, ReadFileTool, WriteFileTool},
    search_tools::SearchFilesTool,
};

/// Build a [`ToolRegistry`] pre-populated with the default built-in tool set.
pub fn build_default_tool_registry(
    workspace_root: impl Into<PathBuf>,
) -> ToolRegistry {
    let workspace_root = workspace_root.into();
    let registry = ToolRegistry::register_defaults();

    // Replace the low-level `synthia_tool` names with the agent-facing names
    // expected by the rest of the system (`read_file`, `write_file`,
    // `search_files`). `apply_patch` keeps the same name but uses the wrapper.
    registry.register(synthia_tool::ToolEntry::new(Arc::new(
        ReadFileTool::default(),
    )));
    registry.register(synthia_tool::ToolEntry::new(Arc::new(
        WriteFileTool::default(),
    )));
    registry.register(synthia_tool::ToolEntry::new(Arc::new(SearchFilesTool)));
    registry.register(synthia_tool::ToolEntry::new(Arc::new(
        ApplyPatchFileTool::default(),
    )));

    let command_manager = Arc::new(CommandManager::new());
    let sandbox = CommandBlacklist::new(workspace_root);
    registry.register(synthia_tool::ToolEntry::new(Arc::new(BashTool::new(
        command_manager,
        sandbox,
    ))));

    registry
}

#[cfg(test)]
mod tests {
    use synthia_core::Registry;

    use super::*;

    #[tokio::test]
    async fn registry_has_file_tools() {
        let registry = build_default_tool_registry("/tmp");
        assert!(
            registry.get("read_file").await.unwrap().is_some(),
            "read_file tool should be registered"
        );
        assert!(
            registry.get("write_file").await.unwrap().is_some(),
            "write_file tool should be registered"
        );
    }

    #[tokio::test]
    async fn registry_has_bash_tool() {
        let registry = build_default_tool_registry("/tmp");
        assert!(
            registry.get("bash").await.unwrap().is_some(),
            "bash tool should be registered"
        );
    }
}
