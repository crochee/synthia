//! Sandbox smoke tests — drive `ShellTool` through its real spawn
//! path with an `ExecutionPolicy` + backend, exactly as a
//! deployment would.
//!
//! Split from the pure unit tests in `src/sandbox.rs`: those pin
//! argv construction and the policy fold; these pin the observable
//! behaviour of a confined run — fail-closed refusals, denial
//! markers, and (when the host can actually run bubblewrap
//! namespaces) a real refused write.
//!
//! Bubblewrap-dependent tests preflight the *capability*, not just
//! the binary: `bwrap --version` succeeding does not mean unshared
//! user namespaces are permitted (WSL2 and hardened hosts refuse
//! them), and a test that asserted a denial on a host that cannot
//! sandbox at all would be pinning a spawn failure, not policy.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, OnceLock},
};

use serde_json::json;
use synthia_tool::{Context, Tool, ToolOutput};
use synthia_tool_shell::{
    BackendAvailability,
    BwrapBackend,
    ExecutionPolicy,
    NoopBackend,
    SANDBOX_UNAVAILABLE,
    SandboxBackend as _,
    ShellTool,
    denial_marker,
    resolve_confined_command,
};

fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(synthia_provider::types::ContentPart::text)
        .collect()
}

fn context_for(ws: &tempfile::TempDir) -> Context {
    Context::new("sandbox-smoke".into(), ws.path().to_path_buf())
}

async fn run(tool: &ShellTool, ctx: &Context, command: &str) -> ToolOutput {
    tool.call(json!({ "command": command, "timeout_secs": 30 }), ctx)
        .await
}

/// A confined policy with no backend refuses before anything is
/// spawned: the output is a typed error carrying
/// `SANDBOX_UNAVAILABLE`, never a `[exit code: …]` marker (which
/// would prove a command ran).
#[tokio::test]
async fn confined_policy_without_backend_fails_closed() {
    let ws = tempfile::tempdir().expect("temp workspace");
    let ctx = context_for(&ws);
    let tool = ShellTool::with_policy(ExecutionPolicy::ReadOnly);
    let out = run(&tool, &ctx, "echo hi").await;

    assert!(
        out.is_error.unwrap_or(false),
        "must be a typed refusal, got: {}",
        text_of(&out)
    );
    let text = text_of(&out);
    assert!(
        text.contains(SANDBOX_UNAVAILABLE),
        "refusal names the code: {text}"
    );
    assert!(
        !text.contains("[exit code:"),
        "nothing may have run: {text}"
    );
}

/// Installing the no-op backend is not a way around a confined
/// policy: it refuses just like a missing backend.
#[tokio::test]
async fn noop_backend_refuses_confined_policy() {
    let ws = tempfile::tempdir().expect("temp workspace");
    let tool = ShellTool::with_policy(ExecutionPolicy::WorkspaceWrite)
        .with_sandbox(Arc::new(NoopBackend::new()));
    let out = run(&tool, &context_for(&ws), "echo hi").await;

    assert!(out.is_error.unwrap_or(false));
    assert!(
        text_of(&out).contains(SANDBOX_UNAVAILABLE),
        "no-op backend refuses confinement: {}",
        text_of(&out)
    );
}

/// The default policy keeps today's byte-for-byte behaviour: no
/// backend needed, the command runs and reports `[exit code: 0]`.
#[tokio::test]
async fn full_access_runs_unconfined() {
    let ws = tempfile::tempdir().expect("temp workspace");
    let out = run(&ShellTool::new(), &context_for(&ws), "printf ok").await;

    let text = text_of(&out);
    assert!(out.is_error.is_none(), "plain run, got: {text}");
    assert!(text.contains("ok"), "stdout captured: {text}");
    assert!(text.contains("[exit code: 0]"), "status marker: {text}");
}

/// Bubblewrap can only be smoke-tested on a host that can actually
/// create its namespaces: run the real wrap of `true` and require a
/// clean exit. A host without unprivileged user namespaces (WSL2,
/// hardened CI) fails here and the tests skip loudly instead of
/// asserting a spawn failure as a policy denial.
fn bwrap_capable(ws: &Path) -> bool {
    static CAPABLE: OnceLock<bool> = OnceLock::new();
    *CAPABLE.get_or_init(|| {
        if !matches!(
            BwrapBackend::new().availability(),
            BackendAvailability::Available
        ) {
            return false;
        }
        let confined = resolve_confined_command(
            Some(&BwrapBackend::new()),
            ExecutionPolicy::ReadOnly,
            ws,
        )
        .expect("available backend confines");
        let argv = confined.argv_for("true");
        let status = Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(status, Ok(s) if s.success())
    })
}

fn skip_unless_bwrap(ws: &tempfile::TempDir) -> bool {
    let capable = bwrap_capable(ws.path());
    if !capable {
        eprintln!("skipping: host cannot run bubblewrap namespaces");
    }
    capable
}

/// Under `ReadOnly` a write into the workspace is refused by the
/// read-only bind of `/`: the run's data is kept, the denial marker
/// names the boundary, the exit marker stays last, and the probe
/// file was never created.
#[tokio::test]
async fn read_only_denies_workspace_write() {
    let ws = tempfile::tempdir().expect("temp workspace");
    if !skip_unless_bwrap(&ws) {
        return;
    }
    let probe = ws.path().join("probe-ro");
    let tool = ShellTool::with_policy(ExecutionPolicy::ReadOnly)
        .with_sandbox(Arc::new(BwrapBackend::new()));
    let out = run(
        &tool,
        &context_for(&ws),
        &format!("touch {}", probe.display()),
    )
    .await;

    let text = text_of(&out);
    assert!(
        text.contains(&denial_marker(ExecutionPolicy::ReadOnly)),
        "denial marker present: {text}"
    );
    assert!(
        !text.contains("[exit code: 0]"),
        "a refused write did not succeed: {text}"
    );
    assert!(
        text.trim_end().ends_with(']'),
        "exit marker stays last: {text}"
    );
    assert!(
        !probe.exists(),
        "the sandbox actually refused the file effect"
    );
}

/// Reads stay unrestricted under `ReadOnly`: a plain write-free
/// command exits 0 with no denial marker.
#[tokio::test]
async fn read_only_allows_reads() {
    let ws = tempfile::tempdir().expect("temp workspace");
    if !skip_unless_bwrap(&ws) {
        return;
    }
    let ctx = context_for(&ws);
    let tool = ShellTool::with_policy(ExecutionPolicy::ReadOnly)
        .with_sandbox(Arc::new(BwrapBackend::new()));
    let out = run(&tool, &ctx, "printf ok").await;

    let text = text_of(&out);
    assert!(text.contains("ok"), "stdout captured: {text}");
    assert!(text.contains("[exit code: 0]"), "runs cleanly: {text}");
    assert!(
        !text.contains("[sandbox:"),
        "no false denial marker: {text}"
    );
}

/// Under `WorkspaceWrite` a write inside the workspace succeeds
/// while a write outside it (to the read-only bind of `/`) is
/// refused with the marker.
#[tokio::test]
async fn workspace_write_scopes_the_grant() {
    let ws = tempfile::tempdir().expect("temp workspace");
    if !skip_unless_bwrap(&ws) {
        return;
    }
    let ctx = context_for(&ws);
    let tool = ShellTool::with_policy(ExecutionPolicy::WorkspaceWrite)
        .with_sandbox(Arc::new(BwrapBackend::new()));

    let inside = run(&tool, &ctx, "touch probe-ww").await;
    let inside_text = text_of(&inside);
    assert!(
        inside_text.contains("[exit code: 0]"),
        "workspace write allowed: {inside_text}"
    );
    assert!(ws.path().join("probe-ww").exists());

    let outside = PathBuf::from("/var/tmp")
        .join(format!("synthia-sandbox-smoke-{}", std::process::id()));
    let denied =
        run(&tool, &ctx, &format!("touch {}", outside.display())).await;
    let denied_text = text_of(&denied);
    assert!(
        denied_text.contains(&denial_marker(ExecutionPolicy::WorkspaceWrite)),
        "outside write denied: {denied_text}"
    );
    let _ = std::fs::remove_file(&outside);
}
