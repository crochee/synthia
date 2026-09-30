//! Tests for the optional integrations the boot wires —
//! MCP server registration (R21/R30).

use synthia::core::{Clock, registry::Registry};

use crate::state::{
    plugin_tool_registry,
    register_configured_mcp_servers,
    tests::resolve_server::EnvContentGuard,
};
/// R21/R30: an enabled MCP server in the config has its
/// tools published into the registry at boot, and its
/// client is retained on the returned list.
#[tokio::test]
async fn boot_registers_configured_mcp_server_tools() {
    let _guard = EnvContentGuard::cleared();
    let script = r#"
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"boot-test","version":"1"}}}'
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"remote_add","description":"Add","inputSchema":{"type":"object"}}]}}'
sleep 5
"#;

    let dir = tempfile::tempdir().unwrap();
    let config = serde_json::json!({
        "version": "1.0",
        "mcp_servers": [{
            "name": "boot-test",
            "command": "sh",
            "args": ["-c", script],
        }],
    });
    std::fs::write(
        dir.path().join("config.toml"),
        serde_json::to_string(&config).unwrap(),
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("the mcp fixture must parse");

    let registry = plugin_tool_registry();
    let (clients, health) = register_configured_mcp_servers(
        &registry,
        synthia::core::SharedClock::system().now(),
        &cfg,
    )
    .await;

    assert_eq!(clients.len(), 1, "one server must register");
    assert_eq!(clients[0].0, "boot-test");
    assert_eq!(clients[0].1.server_name(), "boot-test");
    assert_eq!(health.len(), 1, "one health entry per configured server");
    assert_eq!(health[0].name, "boot-test");
    assert!(health[0].healthy(), "{:?}", health[0]);
    assert_eq!(health[0].registered_tools, 1);

    let entry = registry
        .get("mcp__boot-test__remote_add")
        .await
        .expect("lookup")
        .expect("namespaced remote_add must be registered");
    assert_eq!(entry.tool_instance().name(), "mcp__boot-test__remote_add");
    assert!(
        registry
            .snapshot()
            .iter()
            .any(|m| m.name == "mcp__boot-test__remote_add"),
        "remote tool must appear in the catalog"
    );

    let disabled = serde_json::json!({
        "version": "1.0",
        "mcp_servers": [{
            "name": "off",
            "command": "definitely-not-a-command",
            "enabled": false,
        }],
    });
    std::fs::write(
        dir.path().join("config.toml"),
        serde_json::to_string(&disabled).unwrap(),
    )
    .unwrap();
    let disabled_config =
        crate::config::resolve_server_config(dir.path(), None)
            .expect("config parses");
    let registry2 = plugin_tool_registry();
    let (none, health) = register_configured_mcp_servers(
        &registry2,
        synthia::core::SharedClock::system().now(),
        &disabled_config,
    )
    .await;
    assert!(none.is_empty(), "disabled server must not spawn");
    assert!(
        health.is_empty(),
        "a disabled server is not even probed: {health:?}"
    );
}

/// R21: a server that cannot spawn is skipped, not fatal.
#[tokio::test]
async fn boot_survives_a_broken_mcp_server() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    let config = serde_json::json!({
        "version": "1.0",
        "mcp_servers": [{
            "name": "broken",
            "command": "/nonexistent/mcp-server-binary",
        }],
    });
    std::fs::write(
        dir.path().join("config.toml"),
        serde_json::to_string(&config).unwrap(),
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("the fixture must parse");

    let registry = plugin_tool_registry();
    let (clients, health) = register_configured_mcp_servers(
        &registry,
        synthia::core::SharedClock::system().now(),
        &cfg,
    )
    .await;
    assert!(clients.is_empty(), "broken server must be skipped");
    assert_eq!(health.len(), 1);
    assert_eq!(health[0].name, "broken");
    assert!(!health[0].healthy(), "{:?}", health[0]);
    assert_eq!(health[0].state, synthia::mcp::HealthState::Reconnecting);
    assert!(registry.snapshot().iter().any(|m| m.name == "read"));
}
