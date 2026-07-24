//! Default tool registry construction.
//!
//! Provides a single entry point for building a [`ToolRegistry`] that
//! exposes exactly the built-in tools the agent runtime can execute.

use std::{path::PathBuf, sync::Arc};

use synthia_tool::registry::ToolRegistry;
use synthia_tool_bash::{BashTool, CommandBlacklist, CommandManager};

use crate::{
    control::AgentControl,
    subagent::SubagentSessionFactory,
    tools::{
        agent_tools::{AgentTool, SubagentManager},
        builtins::{
            file_tools::{ApplyPatchFileTool, ReadFileTool, WriteFileTool},
            search_tools::SearchFilesTool,
        },
    },
};

/// Build a [`ToolRegistry`] pre-populated with the default built-in
/// tool set the agent uses in production.
///
/// The registry contains:
/// - `read_file`, `write_file`, `search_files`, `apply_patch` from the
///   agent-facing wrappers in [`crate::tools::builtins`].
/// - `glob`, `grep`, `multi_edit`, `web_fetch` from `synthia_tool::builtin`.
/// - `bash` from `synthia_tool_bash`, wired to a fresh
///   [`CommandManager`] and a [`CommandBlacklist`] scoped to the
///   provided workspace root.
/// - `task` (subagent spawn) only when both `agent_control` and
///   `subagent_session_factory` are provided, indicating the runtime
///   has the full subagent infrastructure available.
pub fn build_default_tool_registry(
    workspace_root: impl Into<PathBuf>,
    agent_control: Option<AgentControl>,
    subagent_session_factory: Option<Arc<dyn SubagentSessionFactory>>,
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

    // Register the subagent task tool only when the runtime is wired
    // with both the control plane and a factory for creating real child
    // sessions.
    if let (Some(_control), Some(_factory)) =
        (agent_control, subagent_session_factory)
    {
        let manager = Arc::new(SubagentManager::new());
        let agent_tool = Arc::new(AgentTool::new(manager, true));
        registry.register(synthia_tool::ToolEntry::new(agent_tool));
    }

    registry
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synthia_core::{Registry, RegistryItem};

    use super::*;

    struct StubFactory;

    #[async_trait::async_trait]
    impl SubagentSessionFactory for StubFactory {
        async fn create_child(
            &self,
            _user_id: String,
            _parent_session_id: String,
            _maybe_id: Option<String>,
            _parent_depth: usize,
        ) -> Result<
            crate::subagent::ChildSessionHandle,
            crate::subagent::SubagentSessionError,
        > {
            Err(crate::subagent::SubagentSessionError::CreationFailed(
                "stub".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn registry_includes_task_tool_when_deps_present() {
        let control =
            AgentControl::new(Arc::new(crate::control::AgentRegistry::new()));
        let registry = build_default_tool_registry(
            "/tmp",
            Some(control),
            Some(Arc::new(StubFactory)),
        );
        let tool = registry.get("task").await.unwrap();
        assert!(tool.is_some(), "task tool should be registered");
        let tool = tool.unwrap();
        assert_eq!(tool.name(), "task");
        assert!(
            tool.description().contains("general:"),
            "description should advertise built-in types"
        );
    }

    #[tokio::test]
    async fn registry_omits_task_tool_when_deps_missing() {
        let registry = build_default_tool_registry("/tmp", None, None);
        assert!(
            registry.get("task").await.unwrap().is_none(),
            "task tool should not be registered without subagent deps"
        );
    }

    #[tokio::test]
    async fn registry_omits_task_tool_when_only_control_present() {
        let control =
            AgentControl::new(Arc::new(crate::control::AgentRegistry::new()));
        let registry = build_default_tool_registry("/tmp", Some(control), None);
        assert!(
            registry.get("task").await.unwrap().is_none(),
            "task tool should require both deps"
        );
    }

    #[tokio::test]
    async fn registry_omits_task_tool_when_only_factory_present() {
        let registry = build_default_tool_registry(
            "/tmp",
            None,
            Some(Arc::new(StubFactory)),
        );
        assert!(
            registry.get("task").await.unwrap().is_none(),
            "task tool should require both deps"
        );
    }
}
