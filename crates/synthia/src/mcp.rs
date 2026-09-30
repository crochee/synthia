//! [`synthia_mcp`] — Model Context Protocol client.
//!
//! [`McpTransport`] is the I/O seam ([`StdioTransport`] for a real
//! child process, [`InMemoryTransport`] for scripted tests);
//! [`McpClient`] speaks the JSON-RPC handshake and `tools/list` /
//! `tools/call`; [`register_mcp_tools`] publishes every remote tool
//! into a local [`ToolRegistry`](crate::tool::ToolRegistry) as an
//! ordinary [`Tool`](crate::tool::Tool), so remote tools inherit the
//! same guards and dispatch path as builtins.
//!
//! Two model-facing paths over that client, and they answer different
//! questions: [`register_mcp_tools`] publishes **one local tool per
//! remote tool** (the catalog is known at assembly time); the `mcp`
//! tool — [`McpControlTool`] + [`register_mcp_control_tool`] — is the
//! **dynamic** path for a catalog that is large, changes mid-run, or
//! was not pre-registered: `servers` / `tools` / `call` over a shared
//! [`McpSupervisor`], so the model reads the live catalog (argument
//! schemas included) and calls a remote tool by its server-side name.
//! Both render through the same projection, so a remote image or an
//! `isError` reply behaves identically either way.
//!
//! The dynamic path is not a way *around* the registry: it refuses (and
//! does not list) a remote tool whose registered entry carries the
//! privacy flag
//! ([`ToolRegistry::set_hidden`](crate::tool::ToolRegistry::set_hidden),
//! what `[tools] hidden = […]` writes), so `synthia::tool`'s
//! "visibility ≠ executability" rule holds
//! on both paths — while a merely deferred tool stays callable, exactly
//! as the registry treats it. It needs the same `Arc<ToolRegistry>` the
//! agent loop dispatches from, because a `ToolRegistry` clone is a deep
//! copy and would observe a different set of registrations.

pub use synthia_mcp::*;
