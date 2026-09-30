//! In-process MCP **server** core: the protocol side
//! [`crate::McpClient`] talks to, served over a
//! [`LoopbackServer`] inbox.
//!
//! The scope is exactly the three methods this crate's client
//! emits — `initialize`, `tools/list`, `tools/call` — plus the
//! `notifications/initialized` ping. That is deliberately not "an
//! MCP server framework": resources, prompts, sampling, and
//! server-initiated requests stay out until something needs them.
//! What it buys is the **protocol boundary**: a process that has
//! tools worth exposing mounts them here and every existing
//! client-side consumer (naming policy, generation swap,
//! `Deferred` exposure, the `mcp` control tool) applies to them
//! unchanged.
//!
//! Typical wiring (see `synthia-server`'s self-management server):
//!
//! ```ignore
//! let (client, server) = loopback_pair("self");
//! let serve = serve(server, ServerInfo::new("self", "1.0"), tools);
//! tokio::spawn(serve);
//! let mcp = McpClient::new(client);
//! mcp.initialize().await?;
//! register_mcp_tools(&registry, mcp, "self", NamingPolicy::default()).await?;
//! ```

use std::{future::Future, sync::Arc};

use futures::future::BoxFuture;
use serde_json::{Value, json};

use crate::{MCP_PROTOCOL_VERSION, McpToolSpec, loopback::LoopbackServer};

/// What the server says about itself at handshake.
#[derive(Clone, Debug)]
pub struct ServerInfo {
    /// Server name (surfaced in logs; also the label callers
    /// usually match their naming policy against).
    pub name: String,
    /// Server version.
    pub version: String,
}

impl ServerInfo {
    /// Build the handshake identity.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

/// The handler signature every [`ServerTool`] carries: arguments
/// in, either the tool's textual output or a message the model
/// should act on as the failure.
pub type ToolHandler = Arc<
    dyn Fn(Value) -> BoxFuture<'static, Result<String, String>> + Send + Sync,
>;

/// One tool an in-process server serves.
#[derive(Clone)]
pub struct ServerTool {
    /// Tool name (namespaced by the client's naming policy).
    pub name: String,
    /// Model-facing description.
    pub description: String,
    /// JSON Schema for the `arguments` object.
    pub input_schema: Value,
    /// The call handler.
    pub handler: ToolHandler,
}

impl ServerTool {
    /// Build a tool from a plain async closure.
    pub fn new<F, Fut>(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: F,
    ) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<String, String>> + Send + 'static,
    {
        let handler = Arc::new(move |args: Value| {
            Box::pin(handler(args))
                as BoxFuture<'static, Result<String, String>>
        });
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            handler,
        }
    }

    /// The `tools/list` entry shape (identical to what a remote
    /// server would send, so client-side consumers see no
    /// difference).
    #[must_use]
    pub fn spec(&self) -> McpToolSpec {
        McpToolSpec {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
        }
    }
}

impl std::fmt::Debug for ServerTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerTool")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

/// Serve `tools` over the loopback inbox until the client side
/// drops.
///
/// The caller owns the task this future runs on — nothing here
/// spawns, so a lib consumer on any runtime (or a test) decides
/// the lifetime. Every envelope gets a response; a handler panic
/// or a malformed envelope degrades to a JSON-RPC error, never a
/// hung request slot.
pub async fn serve(
    mut server: LoopbackServer,
    info: ServerInfo,
    tools: Vec<ServerTool>,
) {
    while let Some(envelope) = server.next_envelope().await {
        let id = envelope.get("id").and_then(Value::as_u64);
        let method =
            envelope.get("method").and_then(Value::as_str).unwrap_or("");
        let params = envelope.get("params").cloned().unwrap_or(Value::Null);
        // Notifications (no id) never get a response — the client
        // would have nowhere to route one.
        let Some(id) = id else {
            continue;
        };
        let response = match method {
            "initialize" => initialize_result(&info),
            "tools/list" => tools_list_result(&tools),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str);
                match name.and_then(|name| {
                    tools.iter().find(|tool| tool.name == name)
                }) {
                    Some(tool) => {
                        let arguments = params
                            .get("arguments")
                            .cloned()
                            .unwrap_or(json!({}));
                        match (tool.handler)(arguments).await {
                            Ok(text) => call_result(text, false),
                            Err(message) => {
                                // Tool-level failure: an envelope
                                // the client decodes, not a dropped
                                // request.
                                call_result(message, true)
                            }
                        }
                    }
                    None => error_envelope(
                        id,
                        -32602,
                        format!(
                            "unknown tool: {}",
                            name.unwrap_or("<missing>")
                        ),
                    ),
                }
            }
            other => {
                error_envelope(id, -32601, format!("method not found: {other}"))
            }
        };
        let mut framed = response;
        if let Some(obj) = framed.as_object_mut() {
            obj.insert("jsonrpc".into(), json!("2.0"));
            obj.insert("id".into(), json!(id));
        }
        server.respond(framed);
    }
}

fn initialize_result(info: &ServerInfo) -> Value {
    json!({
        "result": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {"listChanged": false},
            },
            "serverInfo": {
                "name": info.name,
                "version": info.version,
            },
        }
    })
}

fn tools_list_result(tools: &[ServerTool]) -> Value {
    let specs: Vec<McpToolSpec> = tools.iter().map(ServerTool::spec).collect();
    json!({ "result": { "tools": specs } })
}

fn call_result(text: String, is_error: bool) -> Value {
    json!({
        "result": {
            "content": [{"type": "text", "text": text}],
            "isError": is_error,
        }
    })
}

fn error_envelope(id: u64, code: i64, message: String) -> Value {
    json!({
        "id": id,
        "error": {"code": code, "message": message},
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{McpClient, NamingPolicy, loopback::loopback_pair};

    fn echo_tool() -> ServerTool {
        ServerTool::new(
            "echo",
            "echo the input text",
            json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
            }),
            |args| async move {
                Ok(args
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string())
            },
        )
    }

    fn fail_tool() -> ServerTool {
        ServerTool::new(
            "fail",
            "always fails",
            json!({"type": "object"}),
            |_| async { Err("deliberate failure".to_string()) },
        )
    }

    /// The full client stack works against the in-process server:
    /// handshake, listing (with real schemas), a successful call,
    /// and a tool-level failure surfaced as `isError`.
    #[tokio::test]
    async fn client_round_trips_through_the_server() {
        let (client, server) = loopback_pair("self");
        tokio::spawn(serve(
            server,
            ServerInfo::new("self", "0.1"),
            vec![echo_tool(), fail_tool()],
        ));
        let client = McpClient::new(client);
        let init = client.initialize().await.expect("initialize");
        assert_eq!(init["serverInfo"]["name"], "self");

        let tools = client.tools_list().await.expect("tools/list");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[0].input_schema["type"], "object");

        let echoed = client.tools_call("echo", json!({"text": "hello"})).await;
        let echoed = echoed.expect("echo call");
        assert!(!echoed.is_error);
        assert_eq!(echoed.text(), "hello");

        let failed = client.tools_call("fail", json!({})).await;
        let failed = failed.expect("transport ok, tool failed");
        assert!(failed.is_error);

        let unknown = client.tools_call("nope", json!({})).await;
        assert!(unknown.is_err(), "unknown tool must be an error");
    }

    /// `register_mcp_tools` publishes the served tools into a real
    /// registry under the naming policy — the same path boot uses
    /// for stdio servers, now exercised in-process.
    #[tokio::test]
    async fn registers_into_a_tool_registry() {
        let registry = synthia_tool::ToolRegistry::default();
        let (client, server) = loopback_pair("self");
        tokio::spawn(serve(
            server,
            ServerInfo::new("self", "0.1"),
            vec![echo_tool()],
        ));
        let client = McpClient::new(client);
        client.initialize().await.expect("initialize");
        let generation = crate::register_mcp_tools(
            &registry,
            client,
            "self",
            NamingPolicy::default(),
        )
        .await
        .expect("register");
        assert_eq!(generation.len(), 1);
        let listed = registry.descriptors_cached();
        assert!(
            listed.iter().any(|d| d.name == "mcp__self__echo"),
            "tool must be namespaced, got: {:?}",
            listed.iter().map(|d| d.name.clone()).collect::<Vec<_>>()
        );
    }
}
