//! Tests for `AppState::for_test`'s tool registry wiring —
//! pin the contract that the test surface mirrors the
//! production wiring (the `skill` tool lands on the
//! default registry).

use synthia::{core::registry::Registry, session::manager::SessionRegistry};

use crate::state::AppState;

/// `for_test`'s tool registry MUST include the agent-facing
/// `skill` tool so the test surface mirrors the production
/// wiring. If the registration ever silently regresses
/// (e.g. the call gets moved off the `plugin_tool_registry`
/// path), this test fails loudly instead of letting the LLM
/// silently lose access to skill workflows during
/// route-level testing.
#[tokio::test]
async fn for_test_tool_registry_includes_skill_tool() {
    let dir = tempfile::tempdir().unwrap();
    let session_registry = SessionRegistry::new(dir.path().join("sessions"));
    let state =
        AppState::for_test(session_registry, dir.path().to_path_buf()).await;
    let reg = state.tool_registry.read().await;
    let entry = reg
        .get("skill")
        .await
        .expect("skill lookup must succeed")
        .expect("skill tool must be registered in the default registry");
    assert_eq!(entry.tool_instance().name(), "skill");
}
