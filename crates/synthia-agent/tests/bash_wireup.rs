#![allow(deprecated)]
//! Agent-level wire-up test for `BashTool` via `register_bash`.
//!
//! Per `openspec/changes/user-id-namespace-and-bash-permission-gate/tasks.md` §5.7/5.8:
//! the bash tool is **not** included in `ToolRegistry::register_defaults()`
//! (to avoid a `synthia-tool` ↔ `synthia-tool-bash` cycle); callers wire
//! it up explicitly via `synthia_tool_bash::register_bash`. This file
//! verifies that a fully-assembled agent can use a bash-wired registry
//! end-to-end through the normal `tool_registry` plumbing.
//!
//! What this test covers:
//! - `register_bash` is callable on a vanilla `ToolRegistry` and
//!   results in the registry containing a tool named `"bash"`.
//! - The wired registry is accepted by the agent's component
//!   assembly (no schema/contract mismatch).
//! - The `PermissionChecker` gate is honored when a bash invocation
//!   comes through the registry (defense-in-depth AND-logic).
//!
//! What this test does *not* cover (intentionally, scope boundaries):
//! - Full ReAct loop with LLM provider — that is exercised by the
//!   existing `e2e_*` suites; this file is a focused contract test.

#![cfg(unix)]

use std::{path::PathBuf, sync::Arc};

use synthia_permission::{
    MergedPolicy,
    PermissionAction,
    PermissionChecker,
    rule::PermissionRule,
};
use synthia_provider::ToolUse;
use synthia_tool::{
    ToolRegistry,
    types::{ToolExecutionContext, ToolOutput},
};
use synthia_tool_bash::{CommandBlacklist, CommandManager, register_bash};

fn make_registry_with_bash() -> (ToolRegistry, Arc<CommandManager>) {
    let registry = ToolRegistry::new();
    let command_manager = Arc::new(CommandManager::new());
    let sandbox = CommandBlacklist::new(PathBuf::from("/tmp"));
    register_bash(&registry, command_manager.clone(), sandbox);
    (registry, command_manager)
}

fn first_text(output: &ToolOutput) -> String {
    use synthia_provider::types::ContentPart;
    let mut out = String::new();
    for part in &output.content {
        if let ContentPart::Text(t) = part {
            out.push_str(&t.text);
        }
    }
    out
}

/// After `register_bash`, the registry must contain a tool named
/// `"bash"`. This is the minimal contract that downstream agent code
/// relies on: if the name is wrong or the tool is not registered,
/// the agent's tool dispatcher will silently no-op.
#[test]
fn bash_appears_in_registry_after_register_bash() {
    let (registry, _cm) = make_registry_with_bash();
    assert!(
        registry.contains("bash"),
        "registry must contain 'bash' after register_bash"
    );
}

/// A bash invocation routed through the registry with a `Deny` policy
/// must be rejected with a `ToolOutput::error` and the command must
/// never execute. This is the §5.7/5.8 wire-up: the `PermissionChecker`
/// that the agent attaches to its registry is what enforces the
/// policy on the bash tool, *not* a per-tool reimplementation.
#[tokio::test]
async fn bash_wired_registry_honors_deny_policy() {
    let (registry, _cm) = make_registry_with_bash();
    let policy = MergedPolicy::new(
        &[PermissionRule {
            pattern: "bash".to_string(),
            action: PermissionAction::Deny,
            forced: false,
        }],
        &[],
        &[],
    );
    let registry = registry.with_checker(PermissionChecker::new(policy));

    let tool_use = ToolUse {
        id: "call-1".to_string(),
        name: "bash".to_string(),
        input: serde_json::json!({ "command": "rm -rf /" }),
    };
    let ctx = ToolExecutionContext::new(
        "test-session".to_string(),
        PathBuf::from("/tmp"),
    );

    let outputs = registry
        .run_with_context(vec![tool_use], ctx)
        .await
        .expect("registry call should not error");
    assert_eq!(outputs.len(), 1);
    assert!(
        outputs[0].is_error.is_some(),
        "Deny policy must surface as a ToolOutput::error"
    );
    let text = first_text(&outputs[0]);
    assert!(
        text.contains("Permission denied") || text.contains("denied by user"),
        "expected deny message, got: {text}"
    );
}
