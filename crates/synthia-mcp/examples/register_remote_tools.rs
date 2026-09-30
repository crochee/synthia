//! `register_remote_tools` — MCP lego demonstration (R19 + R30).
//!
//! Spawns two POSIX `sh` children that speak MCP over stdio and
//! publishes both tool surfaces into ONE registry:
//!
//! 1. Two servers advertise the *same* raw tool name (`sum`).
//!    Namespacing (`mcp__<server>__<raw>`) keeps them apart, and
//!    `resolve_prefix` takes a public name back to its server.
//! 2. An [`synthia_mcp::McpSupervisor`] owns the connections and
//!    reports per-server health from `tick(now)`.
//! 3. A `tools/list_changed` re-sync swaps the generation on the
//!    in-memory double — the same path a live notification
//!    takes.
//!
//! No network, no external MCP server required.
//!
//! ```bash
//! cargo run --example register_remote_tools -p synthia-mcp
//! ```

use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use synthia_mcp::{
    InMemoryTransport,
    InMemoryTransportFactory,
    McpSupervisor,
    NamingPolicy,
    StdioConfig,
    StdioTransportFactory,
    public_tool_name,
    resolve_prefix,
};
use synthia_tool::{Context, ToolRegistry};

/// A `sh` responder that answers the MCP handshake, one
/// `tools/list`, and one `tools/call` — all for the server
/// `name`, so two instances can be told apart on the wire.
fn sh_responder(name: &str) -> String {
    format!(
        r#"
read -r line
printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":"2025-06-18","capabilities":{{}},"serverInfo":{{"name":"{name}","version":"1"}}}}}}'
read -r line
read -r line
printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"tools":[{{"name":"sum","description":"Add two integers","inputSchema":{{"type":"object","properties":{{"a":{{"type":"integer"}},"b":{{"type":"integer"}}}},"required":["a","b"]}}}}]}}}}'
read -r line
printf '%s\n' '{{"jsonrpc":"2.0","id":3,"result":{{"content":[{{"type":"text","text":"{name}: 3"}}],"isError":false}}}}'
sleep 5
"#
    )
}

#[tokio::main]
async fn main() {
    println!("== synthia R30: namespaced MCP tools + supervision ==\n");

    // 1. Bring up two servers under one supervisor.
    let registry = ToolRegistry::new();
    let supervisor = McpSupervisor::new();
    for name in ["alpha", "beta"] {
        let config = StdioConfig::new("sh")
            .arg("-c")
            .arg(sh_responder(name))
            .label(name);
        supervisor
            .supervise(name, Arc::new(StdioTransportFactory::new(config)))
            .await;
    }
    let tick = supervisor.tick(&registry, Utc::now()).await;
    for health in &tick.health {
        println!(
            "health    : {:5} state={:?} tools={}",
            health.name, health.state, health.registered_tools
        );
    }

    // 2. Both `sum`s coexist in the registry under distinct names.
    println!("\nboot names:");
    for meta in registry.snapshot() {
        let (server, raw) = resolve_prefix(&meta.name).unwrap_or(("-", "-"));
        println!("  - {}  (server={server}, raw={raw})", meta.name);
    }

    // 3. Call one through the ordinary Tool contract; the wire
    //    still carries the raw name.
    let public = public_tool_name("alpha", "sum");
    assert_eq!(resolve_prefix(&public), Some(("alpha", "sum")));
    let entry = {
        use synthia_core::registry::Registry as _;
        registry.get(&public).await.unwrap().expect("alpha sum")
    };
    let out = entry
        .tool_instance()
        .call(json!({"a": 1, "b": 2}), &Context::default())
        .await;
    println!(
        "\ncall {} : is_error={:?} text={:?}",
        public,
        out.is_error,
        out.content.first().and_then(|p| p.text())
    );

    // 4. Re-sync demo: a `tools/list_changed` notification arrives
    //    for a scripted server, so the registration generation is
    //    fetched and swapped in place.
    let wire = Arc::new(InMemoryTransport::new().with_echo_server());
    let scripted = McpSupervisor::new().with_naming(NamingPolicy::Namespaced);
    scripted
        .supervise(
            "scripted",
            Arc::new(InMemoryTransportFactory::new(Arc::clone(&wire))),
        )
        .await;
    let _ = scripted.tick(&registry, Utc::now()).await;
    wire.set_script(
        "tools/list",
        json!({"tools": [
            {"name": "echo", "description": "Echo the input back"},
            {"name": "reverse", "description": "Reverse a string"}
        ]}),
    );
    scripted
        .notify_list_changed(&registry, "scripted")
        .await
        .expect("re-sync");
    println!("\nafter tools/list_changed:");
    for meta in registry.snapshot() {
        if resolve_prefix(&meta.name).map(|(s, _)| s) == Some("scripted") {
            println!("  - {}", meta.name);
        }
    }

    println!("\n== done ==");
}
