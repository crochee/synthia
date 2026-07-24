//! End-to-end permission gating tests for `BashTool`.
//!
//! Verifies the AND-logic defense-in-depth:
//!   policy Allow ∧ blacklist Allow → execute
//!   anything else → `ToolOutput::error`
//!
//! Tests cover the five scenarios called out in
//! `openspec/changes/user-id-namespace-and-bash-permission-gate/tasks.md` §5.10.
//!
//! Note: `BashTool` is registered into a `ToolRegistry` via
//! `synthia_tool_bash::register_bash` rather than `register_defaults()`
//! to keep the `synthia-tool` ↔ `synthia-tool-bash` dependency
//! direction acyclic (see `lib.rs` in this crate).

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
    Tool,
    ToolRegistry,
    types::{ToolExecutionContext, ToolOutput},
};
use synthia_tool_bash::{
    CommandBlacklist,
    CommandManager,
    bash_tool::BashTool,
    register_bash,
};

fn make_registry_with_bash() -> (ToolRegistry, Arc<CommandManager>) {
    let registry = ToolRegistry::new();
    let command_manager = Arc::new(CommandManager::new());
    let sandbox = CommandBlacklist::new(PathBuf::from("/tmp"));
    register_bash(&registry, command_manager.clone(), sandbox);
    (registry, command_manager)
}

fn make_tool_use(command: &str) -> ToolUse {
    ToolUse {
        id: "call-1".to_string(),
        name: "bash".to_string(),
        input: serde_json::json!({ "command": command }),
    }
}

fn make_context() -> ToolExecutionContext {
    ToolExecutionContext::new("test-session".to_string(), PathBuf::from("/tmp"))
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

/// (a) `Bash("rm -rf /")` is routed through `PermissionChecker` and,
/// when the policy denies it, the registry returns a `ToolOutput::error`
/// without ever invoking the tool.
#[tokio::test]
async fn permission_deny_blocks_before_tool_invocation() {
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

    let outputs = registry
        .run_with_context(vec![make_tool_use("rm -rf /")], make_context())
        .await
        .expect("registry call should not error");

    assert_eq!(outputs.len(), 1);
    assert!(
        outputs[0].is_error.is_some(),
        "denied call must be marked as error, got: {:?}",
        first_text(&outputs[0])
    );
    let text = first_text(&outputs[0]);
    assert!(
        text.contains("Permission denied") || text.contains("denied by user"),
        "expected deny message, got: {text}"
    );
}

/// (b) Calling an unknown tool name (e.g. `BashX`) through the
/// registry returns a "Tool not found" error — `run_with_context` does
/// not crash and does not silently invoke an unregistered tool.
#[tokio::test]
async fn unknown_tool_name_returns_error() {
    let (registry, _cm) = make_registry_with_bash();
    let outputs = registry
        .run_with_context(
            vec![ToolUse {
                id: "call-1".to_string(),
                name: "BashX".to_string(),
                input: serde_json::json!({ "command": "ls" }),
            }],
            make_context(),
        )
        .await
        .expect("registry call should not error");
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error.is_some());
    let text = first_text(&outputs[0]);
    assert!(text.contains("not found"), "got: {text}");
}

/// (c) Even when the policy is `Allow` (or no checker is wired up),
/// the in-tool `CommandBlacklist` still refuses well-known destructive
/// patterns and returns a `ToolOutput::error`.
#[tokio::test]
async fn blacklist_blocks_even_when_policy_allows() {
    let (registry, _cm) = make_registry_with_bash();
    // No checker wired: registry passes the call straight to the tool,
    // where the blacklist gate must catch `rm -rf /`.
    let outputs = registry
        .run_with_context(vec![make_tool_use("rm -rf /")], make_context())
        .await
        .expect("registry call should not error");
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error.is_some());
    let text = first_text(&outputs[0]);
    assert!(
        text.contains("denied by security policy"),
        "expected blacklist block, got: {text}"
    );
}

/// (d) `Bash("echo hello")` clears both gates and returns the actual
/// command output.
#[tokio::test]
async fn echo_clears_both_gates() {
    let (registry, _cm) = make_registry_with_bash();
    let policy = MergedPolicy::new(
        &[PermissionRule {
            pattern: "bash".to_string(),
            action: PermissionAction::Allow,
            forced: false,
        }],
        &[],
        &[],
    );
    let registry = registry.with_checker(PermissionChecker::new(policy));

    let outputs = registry
        .run_with_context(vec![make_tool_use("echo hello")], make_context())
        .await
        .expect("registry call should not error");
    assert_eq!(outputs.len(), 1);
    let text = first_text(&outputs[0]);
    assert!(text.contains("hello"), "expected echo output, got: {text}");
}

/// (e) `BashTool::is_concurrency_safe() == true` is the documented
/// contract; the registry depends on it for fan-out. A regression to
/// `false` would silently serialize all bash invocations and
/// double-bill latency budgets.
#[tokio::test]
async fn is_concurrency_safe_contract_holds() {
    let sandbox = CommandBlacklist::new(PathBuf::from("/tmp"));
    let tool = BashTool::new(Arc::new(CommandManager::new()), sandbox);
    assert!(tool.is_concurrency_safe());
    assert!(tool.requires_permission());
    assert_eq!(tool.name(), "bash");
}
