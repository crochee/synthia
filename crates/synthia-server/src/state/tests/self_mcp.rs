//! Tests for the self-management MCP server the boot wires:
//! the deployment's own management surface, served to its agents
//! over MCP (see `state::app_state::self_mcp`).

use std::sync::Arc;

use synthia::{core::registry::Registry, session::manager::SessionRegistry};

use crate::state::{AppState, app_state::self_mcp};

/// The boot publishes one `mcp__self__<domain>` tool per
/// management domain, with `Deferred` exposure (the same
/// cold-start policy every `mcp__*` tool gets) — and each one is
/// a working MCP client call away from the real `AppState`.
#[tokio::test]
async fn boot_registers_self_management_tools() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Use the `test-utils` constructor: `FakeProvider` sidesteps the
    // requirement that a real LLM provider be configured. The
    // production `with_server_config` path (used by `main.rs`) is
    // gated on `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` being set, which
    // would couple this test to the operator's shell — wrong contract
    // for an in-tree test that asserts the *boot's wiring*, not the
    // LLM resolution.
    let session_manager = SessionRegistry::new(dir.path().join("sessions"));
    let state = Arc::new(
        AppState::for_test(session_manager, dir.path().to_path_buf()).await,
    );
    // `with_server_config` calls `register_self_mcp` for the
    // production boot; the test-utils constructor stops at the
    // shared wiring (registry + provider + skill tool) so the
    // management surface stays under explicit test control.
    self_mcp::register_self_mcp(&state).await;

    let registry = state.tool_registry.read().await;
    let self_tools: Vec<String> = registry
        .descriptors()
        .iter()
        .filter(|d| d.name.starts_with("mcp__self__"))
        .map(|d| d.name.clone())
        .collect();
    for domain in [
        "agents",
        "tools",
        "skills",
        "schedules",
        "workflows",
        "evals",
        "tasks",
        "models",
        "attachments",
    ] {
        let name = format!("mcp__self__{domain}");
        assert!(
            self_tools.contains(&name),
            "missing {name}; registered: {self_tools:?}"
        );
    }
    let deferred = registry
        .descriptors()
        .iter()
        .filter(|d| d.name.starts_with("mcp__self__"))
        .all(|d| d.exposure == synthia::tool::ToolExposure::Deferred);
    assert!(
        deferred,
        "self-management tools must be Deferred like every mcp__* tool"
    );

    // End to end through the whole client stack: the loopback
    // transport, the MCP handshake, and a real dispatch into
    // `AppState`. `models` is the cheapest read that proves it.
    let entry = registry
        .get("mcp__self__models")
        .await
        .expect("lookup")
        .expect("models tool must be registered");
    let tool = entry.tool_instance();
    drop(registry);
    let output: synthia::tool::ToolOutput = tool
        .call(
            serde_json::json!({"action": "list"}),
            &synthia::tool::Context::default(),
        )
        .await;
    let text = format!("{output:?}");
    assert!(
        text.contains("default_provider"),
        "models tool must answer from the workspace config: {text}"
    );
}
