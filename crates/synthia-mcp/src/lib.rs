//! `synthia_mcp` — Model Context Protocol client (R19).
//!
//! Adopted from traitclaw `crates/traitclaw-mcp`
//! (`McpServer::stdio` + `McpTool: ErasedTool` +
//! `McpToolRegistry`), reshaped for synthia's lego seams:
//!
//! - [`McpTransport`] — the I/O seam. Newline-delimited
//!   JSON-RPC 2.0 request/response. Production uses
//!   [`StdioTransport`] (spawns a child process); tests use
//!   [`InMemoryTransport`] with scripted responses, so the
//!   whole client is exercisable with **no process spawn and no
//!   network**.
//! - [`McpClient`] — the protocol layer: `initialize` handshake,
//!   `tools/list`, `tools/call`.
//! - [`McpTool`] — a remote MCP tool exposed as a local
//!   [`synthia_tool::Tool`], so remote tools join the same
//!   registry / dispatch / guard pipeline as builtins.
//! - [`register_mcp_tools`] — publish every remote tool into a
//!   [`synthia_tool::ToolRegistry`] as one [`McpToolGeneration`].
//! - [`McpControlTool`] — the `mcp` tool: the *dynamic* counterpart
//!   of [`McpTool`]. One tool that lists the supervised servers, fetches
//!   one server's live `tools/list`, and calls a remote tool by its
//!   server-side name — for catalogs too large, too volatile, or too
//!   late to pre-register. [`register_mcp_control_tool`] publishes it.
//!   It honours the registry's privacy flag on both actions, so it is a
//!   second *access* path, never a way around the guard.
//! - [`public_tool_name`] / [`NamingPolicy`] — derive stable
//!   `mcp__<server>__<raw>` names so two servers publishing the
//!   same raw tool name coexist.
//! - [`McpSupervisor`] — owns the client generation per server
//!   and reconnects with bounded backoff (clock-injected
//!   [`McpSupervisor::tick`], no timers of its own).
//!
//! ## Why a transport trait?
//!
//! MCP is a *protocol*, not a transport. stdio is the common
//! case ([`StdioTransport`]); [`StreamableHttpTransport`] covers
//! remote streamable-http endpoints; tests use
//! [`InMemoryTransport`]. Supplying a different [`McpTransport`]
//! also makes the protocol layer deterministically testable.
//!
//! ## Wire shape
//!
//! ```text
//! → {"jsonrpc":"2.0","id":1,"method":"initialize","params":{…}}
//! ← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":…}}
//! → {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
//! ← {"jsonrpc":"2.0","id":2,"result":{"tools":[…]}}
//! → {"jsonrpc":"2.0","id":3,"method":"tools/call",
//!    "params":{"name":"echo","arguments":{…}}}
//! ← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":…}]}}
//! ```

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::{Value, json};
use synthia_core::Error;

pub mod client;
pub mod control;
pub mod http;
pub mod loopback;
pub mod naming;
pub mod server;
pub mod stdio;
pub mod supervisor;
pub mod testing;
pub mod tool;

pub use client::{
    McpCallResult,
    McpClient,
    McpContentBlock,
    McpToolGeneration,
    McpToolSpec,
    register_mcp_tools,
    sync_mcp_tools,
};
pub use control::{MCP_TOOL_NAME, McpControlTool, register_mcp_control_tool};
pub use http::{HttpTransportFactory, StreamableHttpTransport};
pub use loopback::{LoopbackServer, LoopbackTransport, loopback_pair};
pub use naming::{NamingPolicy, public_tool_name, resolve_prefix};
pub use server::{ServerInfo, ServerTool, serve};
pub use stdio::{StdioTransport, StdioTransportFactory};
pub use supervisor::{
    HealthState,
    McpSupervisor,
    ReconnectPolicy,
    ServerHealth,
    SupervisorTick,
    TransportFactory,
};
pub use testing::{InMemoryTransport, InMemoryTransportFactory, RecordedCall};
pub use tool::McpTool;

/// MCP protocol revision this client speaks. Servers negotiate
/// down to a revision they support.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// The I/O seam. One JSON-RPC request in, one response out.
///
/// Implementations MUST be safe to call concurrently (the client
/// serialises ids but may issue calls from several tasks).
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a request and await its matching response.
    ///
    /// `id` is chosen by the client; the response's `id` is
    /// validated by the caller, not the transport.
    async fn request(
        &self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, Error>;

    /// Fire-and-forget notification (no response expected).
    async fn notify(&self, method: &str, params: Value) -> Result<(), Error>;

    /// Human-readable description for logs (`"stdio: npx -y …"`).
    fn describe(&self) -> String;
}

/// Build a JSON-RPC 2.0 request envelope.
#[must_use]
pub fn request_envelope(id: u64, method: &str, params: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

/// Build a JSON-RPC 2.0 notification envelope (no `id`).
#[must_use]
pub fn notification_envelope(method: &str, params: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

/// Interpret a JSON-RPC response envelope: `result` on success,
/// a typed error when the server returned `error`.
pub fn unwrap_response(value: Value) -> Result<Value, Error> {
    if let Some(err) = value.get("error") {
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown MCP error");
        return Err(Error::ToolExecution {
            message: format!("MCP error {code}: {message}"),
        });
    }
    Ok(value.get("result").cloned().unwrap_or(Value::Null))
}

/// Spawn configuration for [`StdioTransport`].
#[derive(Clone, Debug)]
pub struct StdioConfig {
    /// Executable to spawn (e.g. `"npx"`, `"python3"`, a path).
    pub command: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Extra environment variables for the child.
    pub env: HashMap<String, String>,
    /// Short label used in logs and error messages, so an operator
    /// sees `server=files` instead of a dumped command line.
    /// Unset → a truncated rendering of the command.
    pub label: Option<String>,
}

impl StdioConfig {
    /// `command` with no args / env.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
            label: None,
        }
    }

    /// Name this transport for logs / errors.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Append an argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append many arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set an environment variable.
    #[must_use]
    pub fn env(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// Type alias for the boxed transport handle most callers hold.
pub type SharedTransport = Arc<dyn McpTransport>;
