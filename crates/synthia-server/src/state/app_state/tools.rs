//! The default tool surface, composed brick by brick from the
//! plugin crates, plus the `AppliedToolSurface` record of what
//! the `[tools]` config section did to it at boot.

use std::sync::Arc;

use synthia::{
    tool::{ToolEntry, ToolRegistry},
    tool_read::ReadTool,
    tool_shell::ShellTool,
    tool_todo::TodoWriteTool,
    tool_web::WebFetchTool,
    tool_write::WriteTool,
};

/// The server's default tool surface, composed brick by brick from
/// the plugin crates — `synthia-tool` itself ships no agent-facing
/// set (only the harness's synthetic contracts and the registry
/// passthrough).
///
/// One line per brick: drop a line to drop a tool, and the plugin
/// crate that backs it leaves the binary with it.
pub(crate) fn plugin_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(ReadTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(WriteTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(ShellTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(TodoWriteTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(WebFetchTool::new())));
    registry
}

/// The effective tool surface after the `[tools]` config section was
/// applied at boot (R34).
///
/// Everything here was actually written to the registry: names the
/// registry does not have, unknown active groups, and group claims
/// that conflicted are dropped into [`skipped`](Self::skipped) after
/// being logged as warnings. `AppState` exposes this as the
/// operator-facing record of what the deployment's config did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedToolSurface {
    /// Tools set to `Deferred` exposure.
    pub deferred: Vec<String>,
    /// Tools hidden through the registry's privacy flag: absent from
    /// every listing *and* refused on dispatch.
    pub hidden: Vec<String>,
    /// Group membership written, in group-name order.
    pub groups: std::collections::BTreeMap<String, Vec<String>>,
    /// Groups activated (always a subset of `groups`).
    pub active_groups: Vec<String>,
    /// Per-request cap installed on every server-built agent.
    pub max_visible: Option<usize>,
    /// Config entries skipped at boot, in encounter order. Each was
    /// logged as a warning; this is the machine-readable record.
    pub skipped: Vec<String>,
}

impl AppliedToolSurface {
    /// The policy that reproduces the configured groups and
    /// `max_visible` on an agent, or `None` when the section declared
    /// neither (keeps the agent on the exact R33 path rather than on
    /// an empty policy that happens to be equivalent).
    #[must_use]
    pub fn policy(&self) -> Option<synthia::tool::ToolSurfacePolicy> {
        if self.groups.is_empty()
            && self.active_groups.is_empty()
            && self.max_visible.is_none()
        {
            return None;
        }
        Some(synthia::tool::ToolSurfacePolicy {
            max_visible: self.max_visible,
            groups: self.groups.clone(),
            active_groups: self.active_groups.clone(),
        })
    }
}
