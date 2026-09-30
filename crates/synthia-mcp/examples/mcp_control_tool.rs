//! `mcp_control_tool` — the model discovers and calls a remote MCP
//! tool through ONE local tool, instead of the host pre-registering the
//! remote catalog as one local tool per remote tool.
//!
//! Spawns a real POSIX `sh` child that speaks MCP over stdio, then
//! drives the `mcp` tool exactly as the agent loop would (`Tool::call`
//! with JSON arguments):
//!
//! 1. `{"action":"servers"}` — which servers exist, and are they up.
//! 2. `{"action":"tools","server":"files"}` — the server's live
//!    catalog, with the argument schema the model needs.
//! 3. `{"action":"call","server":"files","tool":"sum",…}` — invoke it.
//! 4. A tool-level failure (`isError`) arrives as a model-facing error
//!    the model can correct from, not as a transport panic.
//!
//! The contrast with `register_remote_tools` is the point, and it is a
//! contrast in *configuration*, not in reach: `McpSupervisor::tick`
//! always publishes the catalog into whatever registry it is handed.
//! Hand it the agent's registry and every remote tool is pre-registered
//! with a full schema; hand it a scratch registry (this example) and the
//! agent's surface is the single `mcp` tool, with the catalog read on
//! demand — N schemas of context saved, at the cost of one extra round
//! trip before the first call.
//!
//! ```bash
//! cargo run --example mcp_control_tool -p synthia-mcp
//! ```

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use synthia_mcp::{
    McpControlTool,
    McpSupervisor,
    StdioConfig,
    StdioTransportFactory,
    register_mcp_control_tool,
};
use synthia_tool::{Context, Tool, ToolOutput, ToolRegistry};

/// A `sh` response loop: one JSON-RPC answer per *request* (a
/// notification carries no `id`, so the loop skips it). Matching on the
/// method makes the responder indifferent to how many `tools/list`
/// calls the host issues, which is what the run-time discovery path
/// does. `tools/call` with `b == 0` fails, so both outcomes are
/// reachable.
fn sh_responder() -> &'static str {
    r#"
while read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  [ -z "$id" ] && continue
  case "$line" in
    *'"method":"initialize"'*)
      result='{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"files","version":"1"}}' ;;
    *'"method":"tools/list"'*)
      result='{"tools":[{"name":"sum","description":"Add two integers","inputSchema":{"type":"object","properties":{"a":{"type":"integer"},"b":{"type":"integer"}},"required":["a","b"]}},{"name":"divide","description":"Divide a by b","inputSchema":{"type":"object","properties":{"a":{"type":"integer"},"b":{"type":"integer"}},"required":["a","b"]}}]}' ;;
    *'"method":"tools/call"'*)
      case "$line" in
        *'"b":0'*) result='{"content":[{"type":"text","text":"division by zero"}],"isError":true}' ;;
        *) result='{"content":[{"type":"text","text":"42"}],"isError":false}' ;;
      esac ;;
    *) continue ;;
  esac
  printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$result"
done
sleep 5
"#
}

/// The textual projection of a `ToolOutput`.
fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| part.text().map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::main]
async fn main() {
    println!(
        "== the `mcp` tool: discover + call remote tools at run time ==\n"
    );

    // 1. The host supervises one server. `tick` publishes whatever
    //    catalog it finds into the registry it is handed — here a
    //    scratch one, so this agent's surface is the single `mcp` tool
    //    instead of one schema per remote tool. (Tick the agent's real
    //    registry instead and the catalog is pre-registered; that is
    //    `register_remote_tools`' configuration.)
    let scratch = ToolRegistry::new();
    let registry = Arc::new(ToolRegistry::new());
    let supervisor = Arc::new(McpSupervisor::new());
    supervisor
        .supervise(
            "files",
            Arc::new(StdioTransportFactory::new(
                StdioConfig::new("sh")
                    .arg("-c")
                    .arg(sh_responder())
                    .label("files"),
            )),
        )
        .await;
    let tick = supervisor.tick(&scratch, Utc::now()).await;
    println!(
        "server after boot: {:?} ({} tool(s) in its generation)",
        tick.health.first().map(|h| h.state),
        tick.health.first().map(|h| h.registered_tools).unwrap_or(0),
    );
    assert!(register_mcp_control_tool(
        &registry,
        Arc::clone(&supervisor)
    ));
    println!(
        "this agent's tool surface: {:?}",
        registry
            .snapshot()
            .into_iter()
            .map(|meta| meta.name)
            .collect::<Vec<_>>()
    );

    let tool = McpControlTool::new(Arc::clone(&supervisor), &registry);
    let context = Context::default();

    // 2. Discover the servers.
    let servers = tool.call(json!({"action": "servers"}), &context).await;
    println!("\n[servers]\n{}", text_of(&servers));

    // 3. Discover one server's catalog (with schemas).
    let tools = tool
        .call(
            json!({"action": "tools", "server": "files", "tool": "sum"}),
            &context,
        )
        .await;
    println!("\n[tools]\n{}", text_of(&tools));

    // 4. Call a remote tool by its server-side name.
    let sum = tool
        .call(
            json!({
                "action": "call",
                "server": "files",
                "tool": "sum",
                "arguments": {"a": 40, "b": 2}
            }),
            &context,
        )
        .await;
    println!(
        "\n[call sum]\nis_error={:?} text={:?} server={:?}",
        sum.is_error,
        text_of(&sum),
        sum.metadata.get("mcp_server"),
    );
    assert_eq!(sum.is_error, None);
    assert_eq!(text_of(&sum), "42");

    // 5. A remote failure is data the model can act on.
    let divide = tool
        .call(
            json!({
                "action": "call",
                "server": "files",
                "tool": "divide",
                "arguments": {"a": 1, "b": 0}
            }),
            &context,
        )
        .await;
    println!(
        "\n[call divide b=0]\nis_error={:?} text={:?}",
        divide.is_error,
        text_of(&divide),
    );
    assert_eq!(divide.is_error, Some(true));

    // 6. A selector that matches nothing names what does exist.
    let ghost = tool
        .call(json!({"action": "tools", "server": "nope"}), &context)
        .await;
    println!("\n[unknown server]\n{}", text_of(&ghost));
    assert_eq!(ghost.is_error, Some(true));

    // 7. Arguments the tool cannot use come back as a JSON error
    //    instead of being silently dropped.
    let bad: Value =
        json!({"action": "call", "server": "files", "tool": "sum", "arg": {}});
    let rejected = tool.call(bad, &context).await;
    assert_eq!(rejected.is_error, Some(true));

    println!("\nMCP-CONTROL-TOOL: OK");
}
