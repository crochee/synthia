//! `sandbox_policy` — the OS boundary one command runs under, and
//! how a refusal reaches the model.
//!
//! Seam: `synthia_tool_shell::sandbox` — `ExecutionPolicy`,
//! `resolve_confined_command`, `BwrapBackend`, `denial_marker` — as
//! consumed by `synthia_tool_shell::ShellTool`.
//!
//! Look at: the fail-closed fold (a confined policy with no backend
//! refuses instead of running unconfined), the capability probe (a
//! `bwrap --version` that answers is not enough — the host must also
//! permit the unshared namespaces, which WSL2 and hardened hosts
//! refuse), the resolved `bwrap` argv for `ReadOnly`, one allowed
//! read, and one refused write that surfaces as
//! `[sandbox: file access denied under read-only mode]`.
//!
//! Run (no network, no API key; bubblewrap optional):
//!
//! ```bash
//! cargo run -p synthia-tool-shell --example sandbox_policy
//! ```

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use chrono::Utc;
use serde_json::json;
use synthia_tool::{Context, Tool, ToolOutput};
use synthia_tool_shell::{
    BackendAvailability,
    BwrapBackend,
    ExecutionPolicy,
    SANDBOX_UNAVAILABLE,
    SandboxBackend as _,
    ShellTool,
    denial_marker,
    resolve_confined_command,
};

/// A unique workspace under the system temp dir — no `tempfile`
/// dependency, so the example runs on a bare checkout.
fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let dir = std::env::temp_dir()
        .join(format!("synthia-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp workspace");
    dir
}

fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(synthia_provider::types::ContentPart::text)
        .collect()
}

/// Full capability probe: the binary must exist *and* the host must
/// let the wrap create its namespaces, or every denial below would be
/// a spawn failure misread as policy.
fn sandbox_capable(backend: &BwrapBackend, workspace: &Path) -> bool {
    if !backend.availability().is_available() {
        return false;
    }
    let Ok(confined) = resolve_confined_command(
        Some(backend),
        ExecutionPolicy::ReadOnly,
        workspace,
    ) else {
        return false;
    };
    let argv = confined.argv_for("true");
    matches!(
        Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        Ok(status) if status.success()
    )
}

/// The per-call context. The execution policy is **not** here: it
/// belongs to the tool (`ShellTool::with_policy`), so the boundary
/// the model is told about and the one enforced are one value.
fn confined_context(workspace: &Path) -> Context {
    Context::new("sandbox-policy".to_string(), workspace.to_path_buf())
}

async fn run_shell(
    tool: &ShellTool,
    ctx: &Context,
    command: &str,
) -> ToolOutput {
    tool.call(json!({"command": command, "timeout_secs": 30}), ctx)
        .await
}

#[tokio::main]
async fn main() {
    let workspace = temp_workspace("sandbox-policy");
    let backend = BwrapBackend::new();

    println!("== synthia sandbox policy ==\n");
    println!("backend                     : {}", backend.name());
    println!(
        "policy under test           : {}",
        ExecutionPolicy::ReadOnly.as_str()
    );

    // 1. The fail-closed fold, independent of the host: a confined
    //    policy without a backend is a typed refusal carrying
    //    `SANDBOX_UNAVAILABLE`, and `DangerFullAccess` explicitly
    //    bypasses the whole seam.
    let refusal = resolve_confined_command(
        None,
        ExecutionPolicy::WorkspaceWrite,
        &workspace,
    )
    .expect_err("a confined policy with no backend must refuse");
    println!(
        "no backend, workspace-write : fail-closed={} ({refusal})",
        refusal.code() == Some(SANDBOX_UNAVAILABLE)
    );
    let bypass = resolve_confined_command(
        None,
        ExecutionPolicy::DangerFullAccess,
        &workspace,
    )
    .expect("full access needs no backend");
    println!(
        "no backend, full access     : {} {:?}",
        bypass.program.display(),
        bypass.args
    );

    // 2. Capability probe: skip loudly on a host that cannot confine
    //    rather than mislabel a spawn failure as a denial.
    let availability = backend.availability();
    match &availability {
        BackendAvailability::Available => {
            println!("availability                : Available");
        }
        BackendAvailability::Unavailable { reason } => {
            println!("availability                : Unavailable ({reason})");
        }
    }
    if !sandbox_capable(&backend, &workspace) {
        println!("SKIPPED: host cannot sandbox");
        let _ = std::fs::remove_dir_all(&workspace);
        println!("SANDBOX-POLICY: OK");
        return;
    }

    // 3. The argv the shell tool would actually spawn under
    //    `ReadOnly`: a read-only bind of `/`, no network, and the
    //    payload appended by `argv_for`.
    let confined = resolve_confined_command(
        Some(&backend),
        ExecutionPolicy::ReadOnly,
        &workspace,
    )
    .expect("an available backend confines");
    println!(
        "wrap                        : {} {} <cmd>",
        confined.program.display(),
        confined.args.join(" ")
    );

    let ctx = confined_context(&workspace);
    let tool = ShellTool::with_policy(ExecutionPolicy::ReadOnly)
        .with_sandbox(Arc::new(BwrapBackend::new()));

    // 4. Reads stay unrestricted under `ReadOnly`.
    let readable = workspace.join("readable.txt");
    std::fs::write(&readable, "confined read works\n")
        .expect("seed the readable file");
    let allowed =
        run_shell(&tool, &ctx, &format!("cat {}", readable.display())).await;
    let allowed_text = text_of(&allowed);
    println!("\n[read under read-only]");
    println!("{}", allowed_text.trim_end());
    println!(
        "read allowed                : {}",
        allowed_text.contains("confined read works")
    );

    // 5. A write is refused by the read-only bind of `/`; the shell
    //    tool classifies it as a policy denial, not a command failure.
    let denied_path = workspace.join("denied.txt");
    let denied =
        run_shell(&tool, &ctx, &format!("touch {}", denied_path.display()))
            .await;
    let denied_text = text_of(&denied);
    let marker = denial_marker(ExecutionPolicy::ReadOnly);
    println!("\n[write under read-only]");
    println!("{}", denied_text.trim_end());
    println!(
        "denial marker present       : {}",
        denied_text.contains(&marker)
    );
    println!("file was actually created   : {}", denied_path.exists());

    let _ = std::fs::remove_dir_all(&workspace);
    println!("\nSANDBOX-POLICY: OK");
}
