//! Stdio bridge: a small request loop that reads newline-delimited
//! JSON-RPC 2.0 from an [`tokio::io::AsyncRead`], dispatches each
//! envelope through the same protocol core `synthia-mcp` uses
//! internally, and writes the matching responses to a
//! [`tokio::io::AsyncWrite`].
//!
//! ## Why a dedicated pump here?
//!
//! `synthia_mcp::server::serve` is parameterised by a
//! [`synthia_mcp::LoopbackServer`] inbox so it can be exercised
//! against the loopback transport in tests. A stdio server has a
//! different shape: the I/O surface is the process's own stdin /
//! stdout, not a channel pair. Reusing `serve` would force us to
//! stand up the loopback pair, route responses through an
//! `mpsc`, and add an extra hop with no upside.
//!
//! Instead, this module inlines the same protocol core —
//! envelope parsing, the three methods the client emits, and the
//! response envelope shape — and plugs the reader / writer
//! straight into it. The dispatch core is copied from
//! `synthia_mcp::server::serve` so a future change there is
//! reflected by a deliberate review of this file.
//!
//! ## Runtime neutrality (AGENTS.md §3.7)
//!
//! The I/O surface is parameterised on the trait shape every
//! async runtime already implements: any `R: AsyncRead +
//! Unpin`, any `W: AsyncWrite + Unpin`. The production binary
//! hands in `tokio::io::stdin()` / `tokio::io::stdout()`; a test
//! hands in an in-memory pipe from `tokio::io::duplex`; a future
//! consumer could plug in any reader / writer without the lib
//! importing a runtime type. The same reasoning keeps
//! [`ServerSurface`] runtime-neutral too — it carries only
//! `SharedClock` for the schedule tool's "now" and a `PathBuf`
//! for the workspace root.
//!
//! ## Wire
//!
//! ```text
//! → {"jsonrpc":"2.0","id":1,"method":"initialize","params":{…}}
//! ← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":…}}
//! → {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
//! ← {"jsonrpc":"2.0","id":2,"result":{"tools":[…}}
//! → {"jsonrpc":"2.0","id":3,"method":"tools/call",
//!    "params":{"name":"read","arguments":{…}}}
//! ← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text",
//!    "text":"…"}],"isError":false}}
//! ```
//!
//! Every request gets a response. Notifications (no `id`) are
//! accepted and silently dropped — the client would have nowhere
//! to route a response. EOF on the reader exits the loop cleanly.
//!
//! stderr carries operator logs (`tracing`); the writer is
//! reserved for JSON-RPC.

use std::sync::Arc;

use serde_json::{Value, json};
use synthia_mcp::{MCP_PROTOCOL_VERSION, McpToolSpec, ServerInfo, ServerTool};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

use crate::ServerSurface;

/// Drive the binary end-to-end over any async reader / writer.
///
/// `reader` / `writer` are split out so a test harness can swap in
/// in-memory pipes without owning a real TTY. The production
/// `main` passes `tokio::io::stdin()` and `tokio::io::stdout()`.
///
/// Returns when the reader hits EOF (`Ok(())`) or an I/O error
/// (`Err(_)`). The dispatch loop drains one line per call into
/// `BufReader::read_line`; every JSON-RPC envelope with an `id`
/// gets a matching response on the writer.
pub async fn run<R, W>(
    surface: ServerSurface,
    info: ServerInfo,
    reader: R,
    writer: W,
) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let writer = Arc::new(Mutex::new(writer));
    let tools = surface.tools(&info.name);
    let mut stdin = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        let n = match stdin.read_line(&mut line).await {
            Ok(n) => n,
            Err(error) => {
                tracing::warn!(
                    target: "synthia.mcp_server.stdio",
                    %error,
                    "stdin read failed; exiting pump",
                );
                return Err(error);
            }
        };
        if n == 0 {
            // EOF: the caller hung up. Exit cleanly.
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let envelope: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(
                    target: "synthia.mcp_server.stdio",
                    %error,
                    line = %trimmed,
                    "dropping non-JSON line on stdin",
                );
                continue;
            }
        };
        let response = dispatch_request(envelope, &info, &tools).await;
        let Some(response) = response else {
            // Notification: no `id`, no response.
            continue;
        };
        let serialized = match serde_json::to_string(&response) {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(
                    target: "synthia.mcp_server.stdio",
                    %error,
                    "dropping unserializable response",
                );
                continue;
            }
        };
        let mut stdout = writer.lock().await;
        stdout
            .write_all(serialized.as_bytes())
            .await
            .map_err(|error| {
                tracing::warn!(
                    target: "synthia.mcp_server.stdio",
                    %error,
                    "stdout write failed; exiting pump",
                );
                error
            })?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
}

/// Dispatch one envelope through the protocol core. Returns
/// `None` for notifications (the caller drops them silently) and
/// `Some(response)` for requests (the caller writes the
/// response).
///
/// The body mirrors `synthia_mcp::server::serve` line-for-line so
/// the wire shape stays identical to the in-process server.
async fn dispatch_request(
    envelope: Value,
    info: &ServerInfo,
    tools: &[ServerTool],
) -> Option<Value> {
    let id = match envelope.get("id") {
        Some(id) => match id.as_u64() {
            Some(id) => id,
            None => return Some(parse_error()),
        },
        // Notifications: no response.
        None => return None,
    };
    let method = envelope.get("method").and_then(Value::as_str).unwrap_or("");
    let params = envelope.get("params").cloned().unwrap_or(Value::Null);
    let mut response = match method {
        "initialize" => initialize_result(info),
        "tools/list" => tools_list_result(tools),
        "tools/call" => call_dispatch(id, tools, &params).await,
        "notifications/initialized" => {
            // Per spec: client signals it has finished the
            // handshake. No body, no response.
            return None;
        }
        other => {
            error_envelope(id, -32601, format!("method not found: {other}"))
        }
    };
    if let Some(obj) = response.as_object_mut() {
        obj.insert("jsonrpc".into(), json!("2.0"));
        obj.insert("id".into(), json!(id));
    }
    Some(response)
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

/// Dispatch `tools/call`. Returns the full JSON-RPC response
/// envelope (the caller only adds `jsonrpc` + `id`).
///
/// Two error modes:
/// - Protocol errors (unknown tool, missing `name`) → JSON-RPC
///   `error` envelope with `-32602`.
/// - Tool execution errors (handler returns `Err`) → result
///   envelope with `isError: true`, so the model reads the
///   failure instead of treating it as transport noise.
async fn call_dispatch(id: u64, tools: &[ServerTool], params: &Value) -> Value {
    let name = match params.get("name").and_then(Value::as_str) {
        Some(name) => name,
        None => {
            return error_envelope(id, -32602, "missing `name`".to_string());
        }
    };
    let tool = match tools.iter().find(|tool| tool.name == name) {
        Some(tool) => tool,
        None => {
            return error_envelope(id, -32602, format!("unknown tool: {name}"));
        }
    };
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    match (tool.handler)(arguments).await {
        Ok(text) => call_result(text, false),
        Err(message) => {
            tracing::debug!(
                target: "synthia.mcp_server.stdio",
                tool = %tool.name,
                %message,
                "tool returned error: relaying as isError",
            );
            call_result(message, true)
        }
    }
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

/// `-32700` parse-error envelope, returned when the inbound
/// `id` is not a non-negative integer.
fn parse_error() -> Value {
    json!({
        "id": Value::Null,
        "error": {
            "code": -32700,
            "message": "id must be a non-negative integer",
        },
    })
}
