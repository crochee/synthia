//! [`McpControlTool`] — the `mcp` tool: browse and call the remote
//! tools a supervised MCP server advertises.
//!
//! Two ways an MCP server's tools reach the model, and they answer
//! different questions:
//!
//! | path | shape | when to use it |
//! |---|---|---|
//! | [`McpTool`](crate::McpTool) | one local [`Tool`] per discovered remote tool, published by [`register_mcp_tools`](crate::register_mcp_tools) | the host knows the catalog at assembly time and wants each remote schema in the model's own tool list |
//! | [`McpControlTool`] | one `mcp` tool, three actions | the catalog is larger than the context budget, changes while the run is live, or was never pre-registered |
//!
//! Both paths dispatch over the same [`McpClient`] and render through
//! the same projection (the private `tool::output_from_result`), so a
//! remote image survives either way — the control tool only changes
//! *how the call is addressed*.
//!
//! ## What it deliberately cannot do
//!
//! The tool is bound to an [`McpSupervisor`], so it sees exactly the
//! servers the host already supervises. It cannot spawn a process,
//! open a socket, or register a new server: a model-issued call can
//! only read the configured catalog and invoke a tool from it. Adding
//! a server stays a host action ([`McpSupervisor::supervise`]).
//!
//! ## The supervisor must be ticked
//!
//! Every action reads what the host's last
//! [`tick`](McpSupervisor::tick) established — health, live clients,
//! the registered generation. A supervisor that is never ticked
//! reports its servers as `pending` and the tool will refuse every
//! call. The tool does not tick on the caller's behalf: ticking is the
//! host's timer, and a tool that ticked would be a second scheduler
//! beside the host's.
//!
//! ## Governance
//!
//! This is a *second* dispatch path over remote tools, so it honours
//! the same boundary the registry enforces: a remote tool whose
//! registered counterpart carries the privacy flag
//! ([`ToolRegistry::set_hidden`], what a deployment writes with
//! `[tools] hidden = [...]`) is neither listed by `tools` nor callable
//! through `call`. The softer [`ToolExposure::Hidden`] stays
//! advertisement-only, exactly as the registry treats it (see
//! `synthia_tool::registry::dispatch`).
//!
//! [`ToolExposure::Hidden`]: synthia_tool::ToolExposure::Hidden
//!
//! ```
//! use std::sync::Arc;
//!
//! use synthia_mcp::{McpControlTool, McpSupervisor};
//! use synthia_tool::{Tool as _, ToolRegistry};
//!
//! let supervisor = Arc::new(McpSupervisor::new());
//! let registry = Arc::new(ToolRegistry::new());
//! // The tool observes the very registry the agent loop reads, so a
//! // hidden remote tool cannot be reached through it either.
//! let tool = McpControlTool::new(Arc::clone(&supervisor), &registry);
//! assert_eq!(tool.name(), "mcp");
//! ```
//!
//! The tool claims the bare `mcp` name, which the default namespaced
//! policy ([`NamingPolicy::Namespaced`](crate::NamingPolicy::Namespaced))
//! leaves free: a remote tool registers as `mcp__<server>__<tool>`.

use std::{
    collections::HashSet,
    fmt::Write as _,
    sync::{Arc, Weak},
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use synthia_tool::{
    Context,
    Tool,
    ToolEntry,
    ToolOutput,
    ToolRegistry,
    output::{RenderKind, ToolOutputDefinition},
    traits::ExecutionMode,
};

use crate::{
    HealthState,
    McpClient,
    McpSupervisor,
    McpToolSpec,
    tool::output_from_result,
};

/// The name the control tool registers under.
pub const MCP_TOOL_NAME: &str = "mcp";

/// Arguments of one `mcp` call.
///
/// `deny_unknown_fields` mirrors the `additionalProperties: false` in
/// [`McpControlTool::parameters`]: a misspelled key is a model-facing
/// error instead of a silently ignored argument.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpRequest {
    action: String,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    arguments: Option<Value>,
}

/// The `mcp` control tool: `servers` / `tools` / `call` over an
/// [`McpSupervisor`].
pub struct McpControlTool {
    supervisor: Arc<McpSupervisor>,
    /// The registry the agent loop reads, observed through a **weak**
    /// handle. The registry owns this tool, so a strong handle would be
    /// a reference cycle that keeps both alive forever; a weak one
    /// lets the tool consult the deployment's privacy decisions without
    /// participating in ownership. While the tool is dispatching, the
    /// dispatcher itself holds the registry — so the upgrade succeeds
    /// on every call that can actually happen.
    registry: Weak<ToolRegistry>,
}

impl McpControlTool {
    /// Bind the tool to `supervisor`, observing `registry` for the
    /// privacy flags a deployment sets on remote tools.
    ///
    /// `registry` MUST be the same `Arc` the agent loop dispatches
    /// from — a clone taken before the `Arc` was built is a *deep
    /// copy*, so flags set through it would never reach the loop and
    /// vice versa.
    #[must_use]
    pub fn new(
        supervisor: Arc<McpSupervisor>,
        registry: &Arc<ToolRegistry>,
    ) -> Self {
        Self {
            supervisor,
            registry: Arc::downgrade(registry),
        }
    }

    /// The registered name `raw` on `server` carries in the registry,
    /// per the supervisor's naming policy.
    ///
    /// Derived from the *active* policy, not a hardcoded one: a
    /// deployment that configured `NamingPolicy::Raw` registers bare
    /// names, so assuming namespacing would look up a name that does
    /// not exist and silently pass every privacy check.
    fn public_name(&self, server: &str, raw: &str) -> String {
        self.supervisor.naming().apply(server, raw)
    }

    /// The names the deployment marked private.
    ///
    /// One `descriptors()` read answers every name: it applies no
    /// visibility filtering (unlike `snapshot`), carries `is_hidden` per
    /// entry, and is version-cached — so a `tools` listing does not pay
    /// one `Registry::get` per advertised tool.
    ///
    /// A dropped registry means no registrations remain, hence nothing
    /// hidden — the same answer the registry itself would give.
    fn private_names(&self) -> HashSet<String> {
        let Some(registry) = self.registry.upgrade() else {
            return HashSet::new();
        };
        registry
            .descriptors_cached()
            .iter()
            .filter(|descriptor| descriptor.is_hidden)
            .map(|descriptor| descriptor.name.clone())
            .collect()
    }

    /// Whether the deployment marked this remote tool private.
    ///
    /// `is_hidden` is the registry's privacy flag — the one level that
    /// actually refuses dispatch (`ToolExposure::Hidden` is
    /// advertisement-only and stays callable by design, and group /
    /// tier / restriction filters never alter dispatch either). This tool
    /// is a second dispatch path, so it honours exactly the flag
    /// `run_stream` refuses on; without this, `[tools] hidden = […]`
    /// would simply stop being enforceable for remote tools.
    fn is_private(
        &self,
        private: &HashSet<String>,
        server: &str,
        raw: &str,
    ) -> bool {
        private.contains(&self.public_name(server, raw))
    }

    /// Live client for `server`, if the supervisor has one.
    async fn client_for(&self, server: &str) -> Option<Arc<McpClient>> {
        self.supervisor.client(server).await
    }

    /// Why a server has no live client.
    ///
    /// An unknown server and a disconnected one need different
    /// corrections, so they get different messages — and the second
    /// one names the actual state, which is what tells the model
    /// whether to retry or to stop asking.
    async fn refusal(&self, server: &str) -> ToolOutput {
        let health = self.supervisor.health().await;
        match health.iter().find(|h| h.name == server) {
            Some(entry) => ToolOutput::error(format!(
                "MCP server `{server}` is not connected (state: {}); its \
                 tools cannot be called right now. Action `servers` shows \
                 every server's state.",
                state_name(entry.state)
            )),
            None => ToolOutput::error(format!(
                "No MCP server named `{server}` is supervised. Action \
                 `servers` lists the configured servers."
            )),
        }
    }

    /// Every supervised server with its connection state.
    async fn list_servers(&self) -> ToolOutput {
        let health = self.supervisor.health().await;
        if health.is_empty() {
            return ToolOutput::text(
                "No MCP servers are configured for this run.",
            );
        }
        let mut out = String::from("MCP servers:");
        for entry in &health {
            let _ = write!(
                out,
                "\n- {} [{}]: {} tool(s) registered",
                entry.name,
                state_name(entry.state),
                entry.registered_tools,
            );
            if entry.consecutive_failures > 0 {
                let _ = write!(
                    out,
                    ", {} consecutive failure(s)",
                    entry.consecutive_failures,
                );
            }
        }
        out.push_str(
            "\nAction `tools` (with a `server`) lists what one server \
             advertises; action `call` invokes one of those tools.",
        );
        ToolOutput::text(out)
    }

    /// One server's live `tools/list`, or a single entry of it.
    ///
    /// Tools the deployment marked private are filtered out first, not
    /// just skipped on the named-tool path: listing one would advertise
    /// what the deployment hid, and the filter also keeps them out of
    /// the "it advertises: …" correction on a miss.
    async fn list_tools(&self, request: &McpRequest) -> ToolOutput {
        let Some(server) = request.server.as_deref() else {
            return missing_argument("tools", "server");
        };
        let Some(client) = self.client_for(server).await else {
            return self.refusal(server).await;
        };
        let specs = match client.tools_list().await {
            Ok(specs) => specs,
            Err(error) => {
                return ToolOutput::error(format!(
                    "`tools/list` on MCP server `{server}` failed: {error}"
                ));
            }
        };
        let mut visible: Vec<&McpToolSpec> = Vec::with_capacity(specs.len());
        let private = self.private_names();
        for spec in &specs {
            if !self.is_private(&private, server, &spec.name) {
                visible.push(spec);
            }
        }
        let selected: Vec<&McpToolSpec> = match request.tool.as_deref() {
            Some(name) => {
                let found: Vec<&McpToolSpec> = visible
                    .iter()
                    .copied()
                    .filter(|s| s.name == name)
                    .collect();
                if found.is_empty() {
                    return ToolOutput::error(format!(
                        "MCP server `{server}` does not advertise a tool \
                         named `{name}`. It advertises: {}.",
                        advertised_names(&visible)
                    ));
                }
                found
            }
            None => visible,
        };
        if selected.is_empty() {
            return ToolOutput::text(format!(
                "MCP server `{server}` advertises no tools."
            ));
        }
        ToolOutput::text(render_specs(server, &selected))
    }

    /// Invoke one remote tool by its raw server-side name.
    async fn invoke(&self, request: &McpRequest) -> ToolOutput {
        let Some(server) = request.server.as_deref() else {
            return missing_argument("call", "server");
        };
        let Some(tool) = request.tool.as_deref() else {
            return missing_argument("call", "tool");
        };
        if self.is_private(&self.private_names(), server, tool) {
            return ToolOutput::error(format!(
                "Remote tool `{server}/{tool}` is disabled by this \
                 deployment's tool policy; nothing was sent. Pick another \
                 tool from action `tools`.",
            ));
        }
        let Some(client) = self.client_for(server).await else {
            return self.refusal(server).await;
        };
        let arguments = request.arguments.clone().unwrap_or_else(|| json!({}));
        let display = format!("{server}/{tool}");
        match client.tools_call(tool, arguments).await {
            Ok(result) => output_from_result(&result, &display, server),
            Err(error) => ToolOutput::error(format!(
                "MCP tool `{display}` failed: {error}"
            )),
        }
    }
}

#[async_trait]
impl Tool for McpControlTool {
    fn name(&self) -> &str {
        MCP_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Browse and call the tools exposed by the configured Model \
         Context Protocol (MCP) servers. Action `servers` lists the \
         servers and their connection state; `tools` lists one server's \
         live tool catalog with each tool's argument schema; `call` \
         invokes one remote tool by the name `tools` reported. Tools \
         this deployment disabled are not listed and cannot be called. \
         Remote tools the host pre-registered are also callable \
         directly, as `mcp__<server>__<tool>`; this tool is the dynamic \
         path for catalogs that are large, volatile, or not \
         pre-registered."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["servers", "tools", "call"],
                    "description": "`servers` lists the supervised MCP \
                                    servers; `tools` lists one server's \
                                    advertised tools; `call` invokes one \
                                    of them."
                },
                "server": {
                    "type": "string",
                    "description": "Server name, exactly as action \
                                    `servers` reports it. Required by \
                                    `tools` and `call`."
                },
                "tool": {
                    "type": "string",
                    "description": "Remote tool name, exactly as action \
                                    `tools` reports it (the server-side \
                                    name, not the `mcp__<server>__<tool>` \
                                    local name). Required by `call`; \
                                    optional filter for `tools`."
                },
                "arguments": {
                    "type": "object",
                    "description": "Arguments for the remote tool, \
                                    matching the schema action `tools` \
                                    reports. `call` only; defaults to {}."
                }
            },
            "required": ["action"]
        })
    }

    fn mode(&self) -> ExecutionMode {
        // Calls share one transport per server; keep them out of a
        // parallel batch, like the pre-registered MCP proxy tools.
        ExecutionMode::Sequential
    }

    fn output_definition(&self) -> ToolOutputDefinition {
        ToolOutputDefinition::passthrough(MCP_TOOL_NAME)
            .with_kind(RenderKind::Json)
            .with_title("MCP")
    }

    async fn call(&self, input: Value, _context: &Context) -> ToolOutput {
        let request = match serde_json::from_value::<McpRequest>(input) {
            Ok(request) => request,
            Err(error) => {
                return ToolOutput::error(format!(
                    "Invalid arguments: {error}"
                ));
            }
        };
        match request.action.as_str() {
            "servers" => self.list_servers().await,
            "tools" => self.list_tools(&request).await,
            "call" => self.invoke(&request).await,
            other => ToolOutput::error(format!(
                "Unknown `action` `{other}`; expected `servers`, `tools`, \
                 or `call`."
            )),
        }
    }
}

/// Publish the `mcp` control tool into `registry`.
///
/// The supervisor is shared, not cloned: the host keeps driving
/// [`McpSupervisor::tick`] and the tool reads whatever the last tick
/// established. `registry` MUST be the same `Arc` the agent loop
/// dispatches from — it is both the insertion target and the source of
/// the privacy flags the tool honours, and a `ToolRegistry` clone is a
/// deep copy, so a second instance would observe a different set of
/// registrations.
///
/// Returns `true` when the entry was inserted; `false` when a Core tool
/// already occupies the `mcp` name (the registry's immutability guard
/// refuses to overwrite built-in entries with the same name).
pub fn register_mcp_control_tool(
    registry: &Arc<ToolRegistry>,
    supervisor: Arc<McpSupervisor>,
) -> bool {
    let tool = McpControlTool::new(supervisor, registry);
    registry.register_entry(ToolEntry::new(Arc::new(tool)))
}

/// Model-facing message for a required argument that was not supplied.
fn missing_argument(action: &str, argument: &str) -> ToolOutput {
    ToolOutput::error(format!("Action `{action}` requires `{argument}`."))
}

/// Render one server's catalog: name, description, argument schema.
fn render_specs(server: &str, specs: &[&McpToolSpec]) -> String {
    let mut out =
        format!("MCP server `{server}` advertises {} tool(s):", specs.len());
    for spec in specs {
        let schema = serde_json::to_string(&spec.input_schema)
            .unwrap_or_else(|_| "{}".to_string());
        let _ = write!(out, "\n- {}: {}", spec.name, spec.description);
        let _ = write!(out, "\n  arguments: {schema}");
    }
    out
}

/// The advertised names, comma-separated, for a "no such tool" error.
fn advertised_names(specs: &[&McpToolSpec]) -> String {
    if specs.is_empty() {
        return "nothing".to_string();
    }
    specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Stable wire-free name for a [`HealthState`] (the enum has no
/// `Display`; a debug format like `Reconnecting` in a model-facing
/// message reads worse than the lowercase form).
fn state_name(state: HealthState) -> &'static str {
    match state {
        HealthState::Pending => "pending",
        HealthState::Connected => "connected",
        HealthState::Reconnecting => "reconnecting",
        HealthState::Exhausted => "exhausted",
    }
}

#[cfg(test)]
mod tests {
    use super::{advertised_names, state_name};
    use crate::{HealthState, McpToolSpec};

    fn spec(name: &str) -> McpToolSpec {
        McpToolSpec {
            name: name.to_string(),
            description: String::new(),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    #[test]
    fn state_names_are_lowercase() {
        assert_eq!(state_name(HealthState::Pending), "pending");
        assert_eq!(state_name(HealthState::Connected), "connected");
        assert_eq!(state_name(HealthState::Reconnecting), "reconnecting");
        assert_eq!(state_name(HealthState::Exhausted), "exhausted");
    }

    #[test]
    fn advertised_names_joins_or_says_nothing() {
        assert_eq!(advertised_names(&[]), "nothing");
        assert_eq!(
            advertised_names(&[&spec("echo"), &spec("sum")]),
            "echo, sum"
        );
    }
}
