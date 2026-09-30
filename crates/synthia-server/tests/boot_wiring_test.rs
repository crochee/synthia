//! Boot-wiring regression tests (R21).
//!
//! `AppState::new` is the production boot path. Unit tests cover
//! the individual helper `register_configured_mcp_servers`,
//! but only a test that drives
//! `AppState::new` and then the real router proves the wiring:
//! config → spawn → publish → reachable over HTTP.
//!
//! A regression that, say, moved MCP registration after the
//! registry was wrapped in its lock, would pass every unit test
//! and still ship a server with no remote tools. These tests
//! catch that.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

mod support;

use support::install_test_provider;

/// A `sh` child that answers the MCP handshake, advertises one
/// tool, then parks so the client's stdio pipes stay open.
fn responder_script() -> String {
    r#"
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"boot","version":"1"}}}'
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"remote_boot_tool","description":"From a boot-time MCP server","inputSchema":{"type":"object"}}]}}'
sleep 30
"#
    .to_string()
}

/// Build a workspace with a config that enables both boot-time
/// capabilities, then boot through `AppState::new`.
async fn boot_workspace() -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::TempDir::new().unwrap();
    install_test_provider(&temp);

    // `ServerConfig::load` JSON-parses a `.toml` file.
    let config = serde_json::json!({
        "version": "1.0",
        "mcp_servers": [{
            "name": "boot-e2e",
            "command": "sh",
            "args": ["-c", responder_script()],
        }],
    });
    std::fs::write(
        temp.path().join("config.toml"),
        serde_json::to_string(&config).unwrap(),
    )
    .unwrap();

    let state = AppState::new(temp.path().to_path_buf(), None)
        .await
        .expect("boot must succeed");
    (temp, state)
}

/// The whole production chain: config → MCP spawn → tool
/// publication → visible on `GET /api/v1/tools`.
#[tokio::test]
async fn boot_wiring_publishes_mcp_tools_over_http() {
    let (_temp, state) = boot_workspace().await;

    // The client is retained (dropping it would kill the child).
    assert_eq!(state.mcp_clients.len(), 1, "one MCP server registered");
    assert_eq!(state.mcp_clients[0].0, "boot-e2e");
    // R30: boot records per-server health for the operator.
    assert_eq!(state.mcp_health.len(), 1);
    assert!(state.mcp_health[0].healthy(), "{:?}", state.mcp_health[0]);

    let app = create_router(Arc::clone(&state)).await;
    let req = Request::builder()
        .uri("/api/v1/tools")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let names: Vec<&str> = json["data"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    assert!(
        names.contains(&"mcp__boot-e2e__remote_boot_tool"),
        "the namespaced remote MCP tool must be reachable over HTTP: \
         {names:?}"
    );
    // Builtins are still there — registration augments, never
    // replaces, the default surface.
    assert!(names.contains(&"read"), "builtins must survive: {names:?}");
}

/// A deployment with no MCP config boots with the
/// plain builtin surface.
#[tokio::test]
async fn boot_without_config_has_no_remote_tools() {
    let temp = tempfile::TempDir::new().unwrap();
    install_test_provider(&temp);
    let state = AppState::new(temp.path().to_path_buf(), None)
        .await
        .expect("boot must succeed");

    assert!(state.mcp_clients.is_empty());
}

/// R34: the whole `[tools]` chain — config → boot apply → HTTP
/// catalog + agent-side projection. An unknown name (the shape an
/// MCP tool that failed to register leaves behind) must be a warning
/// the boot survives, not a startup failure.
#[tokio::test]
async fn boot_wiring_applies_the_tools_section() {
    let temp = tempfile::TempDir::new().unwrap();
    install_test_provider(&temp);
    let config = serde_json::json!({
        "version": "1.0",
        "tools": {
            "deferred": ["shell", "mcp__missing__ghost"],
            "hidden": ["TodoWrite"],
            "groups": {
                "files": ["read", "write"],
                "net": ["web_fetch"],
            },
            "active_groups": ["files", "never_declared"],
            "max_visible": 3,
        },
    });
    std::fs::write(
        temp.path().join("config.toml"),
        serde_json::to_string(&config).unwrap(),
    )
    .unwrap();

    let state = AppState::new(temp.path().to_path_buf(), None)
        .await
        .expect("an unknown tool name must not fail boot");

    // The applied surface records what was written and what was
    // skipped (both skipped names were logged as warnings).
    assert_eq!(state.tool_surface.deferred, vec!["shell"]);
    assert_eq!(state.tool_surface.hidden, vec!["TodoWrite"]);
    assert_eq!(state.tool_surface.active_groups, vec!["files"]);
    assert_eq!(
        state.tool_surface.skipped,
        vec!["never_declared", "mcp__missing__ghost"],
    );
    assert_eq!(state.tool_surface.max_visible, Some(3));

    {
        let registry = state.tool_registry.read().await;
        assert_eq!(
            registry.exposure("read"),
            Some(synthia::tool::ToolExposure::Direct)
        );
        assert_eq!(
            registry.exposure("shell"),
            Some(synthia::tool::ToolExposure::Deferred)
        );
        assert_eq!(
            registry.exposure("web_fetch"),
            Some(synthia::tool::ToolExposure::Hidden),
            "an inactive group withholds without unregistering"
        );
        assert!(
            registry.contains("web_fetch"),
            "the withheld tool stays callable by the runtime"
        );
    }

    // The HTTP catalog follows: deferred stays, hidden and the
    // inactive group are gone.
    let app = create_router(Arc::clone(&state)).await;
    let req = Request::builder()
        .uri("/api/v1/tools")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let names: Vec<&str> = json["data"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(
        names,
        vec!["read", "search", "shell", "skill", "write"],
        "hidden/deferred/inactive must be reflected over HTTP; the \
         cross-domain `search` tool is part of the default surface"
    );

    // The agent-side projection honours groups + the cap, so a run
    // sees a strictly narrower list than the catalog.
    let registry = Arc::new(state.tool_registry.read().await.clone());
    let policy = state
        .tool_surface
        .policy()
        .expect("groups + cap were configured");
    let agent = synthia::harness::ReActAgent::new(
        Arc::clone(&state.default_provider),
        registry,
    )
    .with_tool_surface(policy);
    let defs = agent.projected_tool_definitions(&[]);
    let projected: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        projected,
        vec!["read", "search", "shell"],
        "active group + ungrouped, capped at three in registry order; \
         `search` now sits in the name-sorted default surface"
    );
}
