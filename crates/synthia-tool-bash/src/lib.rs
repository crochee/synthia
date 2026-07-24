pub mod bash_tool;
pub mod command_blacklist;
pub mod command_manager;
pub mod monitor;

use std::sync::Arc;

pub use bash_tool::{BashTool, TOOL_NAME as BASH_TOOL_NAME};
pub use command_blacklist::CommandBlacklist;
pub use command_manager::CommandManager;
pub use monitor::MonitorTool;
use synthia_tool::{ToolEntry, registry::ToolRegistry};

/// Register a `BashTool` into a `ToolRegistry` so the bash tool is
/// exposed to the agent and routed through the registry's
/// `PermissionChecker`.
///
/// This is a thin convenience wrapper that callers (e.g. the agent
/// component assembly) should invoke when wiring up a fresh
/// `ToolRegistry`. It exists as a separate function (rather than being
/// part of `ToolRegistry::register_defaults`) because `synthia-tool-bash`
/// already depends on `synthia-tool` (for the `Tool` trait); inverting
/// the dependency to put `BashTool` inside the default registry would
/// create a circular crate dependency. Keeping the wiring explicit at
/// the call site makes the dependency direction unambiguous.
pub fn register_bash(
    registry: &ToolRegistry,
    command_manager: Arc<CommandManager>,
    sandbox: CommandBlacklist,
) {
    registry.register(ToolEntry::new(Arc::new(BashTool::new(
        command_manager,
        sandbox,
    ))));
}
