//! Construction helpers for the production [`ToolOrchestrator`].
//!
//! This module centralises how built-in tools and bash are assembled into a
//! single resolver that the agent runtime executes through
//! [`DefaultToolOrchestrator`]. MCP tools are registered separately by the
//! caller after discovery (see [`crate::component_assembly`]).

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use synthia_permission::ApprovalService;
use synthia_sandbox::SandboxManager;
use synthia_tool_orchestrator::{
    DefaultToolOrchestrator,
    DynamicResolver,
    ExecutableTool,
    RetryPolicy,
    ToolOrchestrator,
    ToolResolver,
    default_tool_resolver,
};

use super::builtins::bash_tool;

/// Build a [`DynamicResolver`] containing the default built-in tools and
/// bash.
pub fn build_default_tool_resolver(
    workspace_root: impl Into<PathBuf>,
) -> DynamicResolver {
    let workspace_root = workspace_root.into();
    let mut tools: HashMap<String, Arc<dyn ExecutableTool>> =
        default_tool_resolver().into_tools();

    if let Some(bash) = bash_tool(workspace_root) {
        tools.insert("bash".to_string(), bash);
    }

    DynamicResolver::with_tools(tools)
}

/// Build the production [`ToolOrchestrator`] used by the agent runtime.
///
/// The returned orchestrator executes tools through approval, sandbox
/// selection, retry, and lifecycle-event emission. Callers supply the
/// [`ApprovalService`] and [`SandboxManager`] implementations appropriate
/// to their environment (CLI vs server).
pub fn build_default_tool_orchestrator(
    workspace_root: impl Into<PathBuf>,
    approval_service: Arc<dyn ApprovalService>,
    sandbox_manager: Arc<dyn SandboxManager>,
) -> (Arc<dyn ToolOrchestrator>, Arc<DynamicResolver>) {
    let resolver = Arc::new(build_default_tool_resolver(workspace_root));
    let resolver_dyn: Arc<dyn ToolResolver> = resolver.clone();
    let orchestrator = Arc::new(DefaultToolOrchestrator::new(
        resolver_dyn,
        approval_service,
        sandbox_manager,
        RetryPolicy::default(),
    ));
    (orchestrator, resolver)
}
