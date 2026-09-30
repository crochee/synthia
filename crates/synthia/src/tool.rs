//! [`synthia_tool`] — the tool seam and the registry: the paradigm
//! every tool plugin implements.
//!
//! Implement [`Tool`] (four methods; `#[derive(Tool)]` from
//! [`crate::macros`] generates them), register it in a
//! [`ToolRegistry`], and the agent loop dispatches it with a
//! [`Context`] and a [`ToolOutput`] contract. Wrapping registries
//! change *what the model sees* without changing what the registry
//! can execute: [`AdaptiveRegistry`] (tier cap), [`GroupedRegistry`]
//! (named groups), [`RestrictedRegistry`] (allow/deny lists),
//! [`ToolExposure`] (deferred schemas), plus the `workspace` path
//! confinement a file-touching tool enforces.
//!
//! The agent-facing tool **implementations** are separate plugin
//! crates, each behind its own feature: `synthia::tool_read`,
//! `synthia::tool_write`, `synthia::tool_shell`, `synthia::tool_todo`,
//! `synthia::tool_web`, `synthia::tool_task`, `synthia::tool_scheduler`
//! and `synthia::tool_search`. This crate ships the
//! paradigm and the two synthetic contracts the harness injects itself
//! (`__get_full_output`, `structured_output`) — not the parts. A
//! boundary a tool needs beyond a path is the plugin's own:
//! `synthia::tool_shell` carries the OS execution policy
//! (`ExecutionPolicy` + `SandboxBackend`) because it is the plugin
//! that runs processes; `synthia::tool_search` carries the
//! `Registry` it fans a query across.

pub use synthia_tool::*;
