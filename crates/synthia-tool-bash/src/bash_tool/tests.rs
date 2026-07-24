//! 13 unit tests for the `bash_tool` module family.
//!
//! Coverage map:
//!
//! - [`super::BashTool`]: 7 tests (name /
//!   requires_permission / is_concurrency_safe / echo /
//!   timeout / failed_command / empty_command /
//!   blacklist_blocks_dangerous_command).
//! - [`super::cap_to_char_boundary`]: 5 tests
//!   (chinese_3byte_mid_character /
//!   emoji_4byte_mid_character /
//!   mixed_multibyte / ascii_no_adjustment /
//!   max_bytes_larger_than_input).

use std::sync::Arc;

use synthia_sandbox::SandboxAttempt;
use synthia_tool::{Tool, ToolInput, types::ToolExecutionContext};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    command_blacklist::CommandBlacklist,
    command_manager::CommandManager,
};

fn make_tool() -> BashTool {
    BashTool::new(
        Arc::new(CommandManager::new()),
        CommandBlacklist::new(std::path::PathBuf::from("/tmp")),
    )
}

fn make_input(command: &str) -> ToolInput {
    ToolInput {
        name: TOOL_NAME.to_string(),
        input: serde_json::json!({ "command": command }),
        context: ToolExecutionContext::new(
            "test-session".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    }
}

#[test]
fn test_bash_tool_name() {
    let tool = make_tool();
    assert_eq!(tool.name(), "bash");
}

#[test]
fn test_bash_tool_requires_permission() {
    let tool = make_tool();
    assert!(tool.requires_permission());
}

#[test]
fn test_bash_tool_is_concurrency_safe() {
    let tool = make_tool();
    // Documented contract: bash is concurrency-safe at the
    // tool-instance level. Per-invocation shared state would force
    // this to false.
    assert!(tool.is_concurrency_safe());
}

#[tokio::test]
#[cfg(unix)]
async fn test_bash_echo() {
    let tool = make_tool();
    let output = tool.call(make_input("echo hello")).await;
    assert!(output.is_error.is_none());
    // First content part should be text containing "hello".
    let text = format!("{:?}", output.content);
    assert!(text.contains("hello"), "got: {text}");
}

#[tokio::test]
#[cfg(unix)]
async fn test_bash_timeout() {
    let tool = make_tool().with_default_timeout(1);
    let output = tool.call(make_input("sleep 5")).await;
    assert!(output.is_error.is_some());
    let text = format!("{:?}", output.content);
    assert!(text.contains("timed out"), "got: {text}");
}

#[tokio::test]
#[cfg(unix)]
async fn test_bash_failed_command() {
    let tool = make_tool();
    let output = tool.call(make_input("exit 42")).await;
    // exit 42 is a non-zero exit. The old API returned a text
    // result with the prefix `Exit code: 42`. The new API keeps
    // that convention so existing callers / tests stay aligned.
    let text = format!("{:?}", output.content);
    assert!(text.contains("Exit code: 42"), "got: {text}");
}

#[tokio::test]
async fn test_bash_empty_command_returns_error() {
    let tool = make_tool();
    let output = tool.call(make_input("")).await;
    assert!(output.is_error.is_some());
}

#[tokio::test]
#[cfg(unix)]
async fn test_bash_blacklist_blocks_dangerous_command() {
    // The CommandBlacklist is the in-tool defense-in-depth gate.
    // Even if the policy is mis-configured to allow, the blacklist
    // still refuses the well-known destructive patterns and
    // returns an error result.
    let tool = make_tool();
    let output = tool.call(make_input("rm -rf /")).await;
    assert!(output.is_error.is_some());
    let text = format!("{:?}", output.content);
    assert!(text.contains("denied by security policy"), "got: {text}");
}

// ===== cap_to_char_boundary unit tests =====
// These tests cover the UTF-8 safe truncation contract defined in
// openspec/changes/compact-truncate-prune-convergence/specs/bash-utf8-safe-truncate/spec.md

#[test]
fn cap_to_char_boundary_chinese_3byte_mid_character() {
    // "你好世界" = each char 3 bytes UTF-8, 12 bytes total
    let mut s = String::from("你好世界");
    // 7 bytes falls in middle of 3rd char (bytes 6..9), should round down to 6
    cap_to_char_boundary(&mut s, 7);
    assert_eq!(s, "你好");
    // Verify still valid UTF-8
    assert!(std::str::from_utf8(s.as_bytes()).is_ok());
}

#[test]
fn cap_to_char_boundary_emoji_4byte_mid_character() {
    // 😀 = 4 bytes UTF-8 (F0 9F 98 80), 😀😀 = 8 bytes
    let mut s = String::from("😀😀");
    // 5 bytes falls in middle of 2nd emoji (bytes 4..8), should round down to 4
    cap_to_char_boundary(&mut s, 5);
    assert_eq!(s, "😀");
    assert!(std::str::from_utf8(s.as_bytes()).is_ok());
}

#[test]
fn cap_to_char_boundary_mixed_multibyte() {
    // "Hi你好😀" = "Hi"(2) + "你"(3) + "好"(3) + "😀"(4) = 12 bytes
    let mut s = String::from("Hi你好😀");
    // 6 bytes: "Hi"(2) + "你"(3) + part of "好", should round down to 5
    cap_to_char_boundary(&mut s, 6);
    assert_eq!(s, "Hi你");
    assert!(std::str::from_utf8(s.as_bytes()).is_ok());
}

#[test]
fn cap_to_char_boundary_ascii_no_adjustment() {
    let mut s = String::from("Hello, World!");
    cap_to_char_boundary(&mut s, 5);
    assert_eq!(s, "Hello");
}

#[test]
fn cap_to_char_boundary_max_bytes_larger_than_input() {
    let mut s = String::from("你好");
    cap_to_char_boundary(&mut s, 1000);
    // No-op: should not panic and string unchanged
    assert_eq!(s, "你好");
}

// ===== Sandbox wiring tests (U1 single-point-of-failure fix) =====
// These cover the spec at
// openspec/changes/adversarial-audit-p0-fixes/specs/bash-sandbox-application/spec.md:
// the bash executor MUST call `SandboxAttempt::wrap` before executing, and
// MUST deny (not bare-run) when the selected sandbox is unavailable.

/// Scenario: Sandbox Available Wraps Command.
///
/// `build_bash_command` is the seam between "construct a bash -c command" and
/// "execute it". Asserting the rewritten command's program is `bwrap` and its
/// args contain `--unshare-all`/`--die-with-parent` proves `wrap` was invoked
/// without depending on bwrap being installed on the test host.
#[test]
#[cfg(unix)]
fn test_bash_executor_calls_sandbox_wrap_when_available() {
    let sandbox = SandboxAttempt::Bubblewrap {
        workspace: std::path::PathBuf::from("/tmp"),
        args: vec![],
    };
    let cmd = BashTool::build_bash_command("echo hello", &sandbox)
        .expect("wrap with an available sandbox must succeed");

    assert_eq!(
        cmd.as_std().get_program(),
        std::ffi::OsStr::new("bwrap"),
        "wrap must rewrite the program to bwrap"
    );
    let args: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert!(
        args.contains(&"--unshare-all".to_string()),
        "bwrap args must contain --unshare-all, got: {args:?}"
    );
    assert!(
        args.contains(&"--die-with-parent".to_string()),
        "bwrap args must contain --die-with-parent, got: {args:?}"
    );
    assert!(
        args.contains(&"--ro-bind".to_string()),
        "bwrap args must contain --ro-bind (read-only bind of host system dirs per spec Scenario 1), got: {args:?}"
    );
    assert!(
        args.contains(&"--".to_string()),
        "bwrap args must separate sandbox flags from the program via --, got: {args:?}"
    );
    // After `--`, bwrap must preserve the original `bash -c <command>`
    // invocation unchanged: `bash` (program), `-c` (flag), and the full
    // shell command string as a single arg. The command string is NOT
    // word-split — `bash -c "echo hello"` passes `"echo hello"` as one
    // arg, which is the standard `bash -c` contract.
    assert!(
        args.contains(&"bash".to_string()),
        "original program `bash` must be preserved after --, got: {args:?}"
    );
    assert!(
        args.contains(&"-c".to_string()),
        "original `-c` flag must be preserved after --, got: {args:?}"
    );
    assert!(
        args.contains(&"echo hello".to_string()),
        "original command string must be preserved as a single arg after --, got: {args:?}"
    );
}

/// Scenario: Sandbox Unavailable Denies By Default.
///
/// When the orchestrator selected a real sandbox (Standard policy) but the
/// backend is unavailable, `SandboxAttempt::Unavailable` is passed. The bash
/// tool MUST fail-closed (P6): return an error result instead of executing
/// with parent-process privileges. This test does not depend on bwrap being
/// installed — it only asserts the deny path.
#[tokio::test]
#[cfg(unix)]
async fn test_bash_executor_denies_when_sandbox_unavailable_standard_policy() {
    let tool = make_tool();
    let input = make_input("echo hello");
    let sandbox = SandboxAttempt::Unavailable;

    let output = tool
        .call_with_sandbox(input, &sandbox, &CancellationToken::new())
        .await;

    assert!(
        output.is_error.unwrap_or(false),
        "sandbox unavailable + Standard policy MUST deny, got: {:?}",
        output.content
    );
    let text = format!("{:?}", output.content);
    assert!(
        text.to_lowercase().contains("sandbox"),
        "error message must indicate sandbox unavailable, got: {text}"
    );
    assert!(
        !text.contains("hello"),
        "command MUST NOT have executed under parent privileges, got: {text}"
    );
}

/// Scenario: Background Spawn Applies Sandbox.
///
/// Spec: bash-sandbox-application Scenario 4 — when `run_in_background` is
/// true, the bash tool MUST still route through `build_bash_command` →
/// `SandboxAttempt::wrap`. A detached subprocess without `--unshare-all` /
/// `--die-with-parent` is the worst-case escape hatch (survives parent,
/// keeps full filesystem write access). This test proves the background
/// path denies (not bare-spawns) when the selected sandbox is
/// `Unavailable`: if the path skipped `wrap`, the command would spawn
/// successfully under parent privileges and return a background ID instead
/// of an error. The command string is irrelevant for the deny case, but a
/// short `sleep` is used so a regression cannot leak a long-lived process.
#[tokio::test]
#[cfg(unix)]
async fn test_background_spawn_applies_sandbox_wrap() {
    let tool = make_tool();
    let input = ToolInput {
        name: TOOL_NAME.to_string(),
        input: serde_json::json!({
            "command": "sleep 1",
            "run_in_background": true,
        }),
        context: ToolExecutionContext::new(
            "test-session".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };
    let sandbox = SandboxAttempt::Unavailable;

    let output = tool
        .call_with_sandbox(input, &sandbox, &CancellationToken::new())
        .await;

    assert!(
        output.is_error.unwrap_or(false),
        "background spawn with Unavailable sandbox MUST deny via wrap, got: {:?}",
        output.content
    );
    let text = format!("{:?}", output.content);
    assert!(
        text.to_lowercase().contains("sandbox"),
        "error message must indicate sandbox could not be applied, got: {text}"
    );
    // The command MUST NOT have bare-spawned — if `wrap` were skipped, the
    // background path would return "Command started in background. ID:"
    // instead of an error. Absence of that marker proves the deny path ran.
    assert!(
        !text.contains("Command started in background"),
        "background command MUST NOT have bare-spawned under parent privileges, got: {text}"
    );
}
