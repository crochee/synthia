//! Self-contained `shell` tool plugin. It depends only on the
//! `synthia-tool` paradigm crate for the `Tool` trait, its context
//! and output types; process confinement lives in this crate's own
//! [`sandbox`] module, so a consumer that never registers `shell`
//! carries none of it.
//!
//! `shell` built-in tool — execute a shell command.
//!
//! Safety contract (per `mvp-shell-safety` spec):
//! - Default deny patterns block obviously destructive commands.
//! - The check can be disabled via [`ShellSafetyConfig::disabled`].
//!
//! Execution contract (dsh / deepseek-harness aligned):
//! - The command runs in `sh -c` with [`Context::workspace_root`]
//!   as its working directory, in its **own process group**, so a
//!   timeout can take down the whole spawned tree.
//! - `timeout_secs` defaults to 120 s and is clamped to
//!   `[1, 600]` — the model cannot ask for an unbounded run.
//! - On timeout the tree gets `SIGTERM`, a 3 s grace, then
//!   `SIGKILL` (no orphaned grandchildren).
//! - Environment is hardened against interactive/paged output
//!   (`NO_COLOR`, `TERM=dumb`, `PAGER=cat`, `GIT_PAGER=cat`).
//! - A **non-zero exit is a normal result**, not an error: the
//!   model needs failing output as data (failing tests, probe
//!   commands). The exit status is rendered as a parseable
//!   trailing marker, always LAST:
//!   `[exit code: N]` / `[timed out after Ns]` /
//!   `[killed by signal: N]`.
//!
//! Sandbox contract (R31, dsh parity) — owned by this crate:
//! - The tool holds its own [`ExecutionPolicy`] and, for a
//!   confined policy, the [`SandboxBackend`] that enforces it
//!   ([`ShellTool::with_policy`] / [`ShellTool::with_sandbox`]),
//!   so the description it shows the model and the argv a call
//!   spawns always come from one value.
//! - [`resolve_confined_command`] turns that pair into the argv:
//!   under [`ExecutionPolicy::DangerFullAccess`] it is `sh -c`
//!   exactly as before, under a confined policy the backend's
//!   wrap.
//! - A confined policy with no usable backend is a **typed
//!   refusal** (`SANDBOX_UNAVAILABLE`) — the tool never degrades
//!   into an unconfined run.
//! - A file effect the sandbox refused is rendered as the
//!   `[sandbox: file access denied under <mode> mode]` marker
//!   (still followed by the exit marker), so the model can tell a
//!   policy boundary from a command that merely failed.

use std::{
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use schemars_derive::JsonSchema;
use serde::Deserialize;
use synthia_tool::{
    Context,
    ExecutionMode,
    RenderKind,
    Tool,
    ToolAnnotations,
    ToolOutput,
    ToolOutputDefinition,
    signal_of,
};
use tokio::{io::AsyncReadExt, process::Command};

pub mod sandbox;
pub use sandbox::{
    BackendAvailability,
    BwrapBackend,
    ConfinedCommand,
    ExecutionPolicy,
    KNOWN_DENIAL_SIGNATURES,
    NoopBackend,
    SANDBOX_UNAVAILABLE,
    SandboxBackend,
    SandboxError,
    bwrap_profile_args,
    classify_denial,
    denial_dialect,
    denial_marker,
    effective_policy,
    policy_prompt_block,
    resolve_confined_command,
};

/// `shell`'s contract text, before the execution-policy paragraph
/// [`policy_prompt_block`] appends.
const BASE_DESCRIPTION: &str = "Execute a shell command and return its stdout and stderr \
     with a trailing exit-status marker. Non-zero exits are \
     normal results — inspect the `[exit code: N]` marker.";

/// Default deny-pattern list. Substring match (case-sensitive).
pub const DEFAULT_DENY_PATTERNS: &[&str] = &[
    "rm -rf",
    "mkfs",
    "dd if=",
    "chmod -R 777",
    "chmod -R 7777",
    "sudo ",
    "curl ",
    "wget ",
    ">/dev/sd",
    ":(){:|:&};:",
];

/// Default execution timeout (dsh parity: 120 s).
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Hard upper bound on a model-requested timeout (dsh parity:
/// 600 s). Larger values are clamped, never rejected — the model
/// should not be able to wedge a session for hours.
pub const MAX_TIMEOUT_SECS: u64 = 600;

/// Grace period between `SIGTERM` and `SIGKILL` when a command
/// overruns its timeout (dsh parity: 3 s).
const KILL_GRACE_SECS: u64 = 3;

#[derive(Debug, Clone, Default)]
pub struct ShellSafetyConfig {
    pub disabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
struct ShellArgs {
    #[schemars(
        description = "The shell command to execute (passed to `sh -c`)."
    )]
    command: String,
    #[serde(default)]
    #[schemars(
        range(min = 1),
        extend("default" = 120),
        description = "Maximum execution time in seconds. Default: 120, \
                       capped at 600."
    )]
    timeout_secs: Option<u64>,
}

#[derive(Debug)]
pub struct ShellTool {
    config: ShellSafetyConfig,
    /// File-effect boundary every call runs under. The description
    /// shown to the model is derived from this same value, so the
    /// advertised policy and the enforced one cannot disagree.
    policy: ExecutionPolicy,
    /// Backend that enforces [`ShellTool::policy`] when it is not
    /// `DangerFullAccess`. `None` with a confined policy is a
    /// fail-closed refusal at call time, never a silent unconfined
    /// run.
    sandbox: Option<Arc<dyn SandboxBackend>>,
    description: String,
}

impl ShellTool {
    /// Shell tool with the default safety config, running under
    /// the default (unconfined) execution policy.
    pub fn new() -> Self {
        Self::with_config(ShellSafetyConfig::default())
    }

    /// Shell tool with `config` under the default policy.
    pub fn with_config(config: ShellSafetyConfig) -> Self {
        Self::with_config_and_policy(config, ExecutionPolicy::default())
    }

    /// Shell tool running under `policy`.
    ///
    /// The policy is the tool's own configuration: the description
    /// the model is shown and the argv a call spawns both come
    /// from it. A confined policy needs
    /// [`ShellTool::with_sandbox`]; without a backend the call is
    /// refused (`SANDBOX_UNAVAILABLE`) rather than run unconfined.
    pub fn with_policy(policy: ExecutionPolicy) -> Self {
        Self::with_config_and_policy(ShellSafetyConfig::default(), policy)
    }

    /// Shell tool with both a safety config and a policy.
    pub fn with_config_and_policy(
        config: ShellSafetyConfig,
        policy: ExecutionPolicy,
    ) -> Self {
        let description =
            format!("{BASE_DESCRIPTION}\n\n{}", policy_prompt_block(policy));
        Self {
            config,
            policy,
            sandbox: None,
            description,
        }
    }

    /// Attach the backend that enforces this tool's policy.
    ///
    /// The backend is consulted only under a confined policy; under
    /// the default (`danger-full-access`) the run is unconfined by
    /// construction and no backend is needed.
    pub fn with_sandbox(mut self, sandbox: Arc<dyn SandboxBackend>) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    fn is_denied(&self, command: &str) -> bool {
        if self.config.disabled {
            return false;
        }
        DEFAULT_DENY_PATTERNS
            .iter()
            .any(|pattern| command.contains(pattern))
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

/// A completed shell run: collected streams plus the cause-of-death
/// classification. `timed_out` and `signal` are mutually exclusive
/// with a plain exit code (first-cause wins, dsh parity).
struct ShellRun {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    signal: Option<i32>,
    timed_out: bool,
    /// Set when the sandbox refused a file effect — the exit code
    /// stays authoritative, the marker only names the boundary.
    denied_by_policy: Option<ExecutionPolicy>,
}

impl ShellRun {
    /// Render the wire body: stdout, stderr, then the status
    /// marker — always LAST so `parse_exit_status`-style consumers
    /// can anchor on the tail.
    fn render(&self, timeout_secs: u64) -> String {
        let mut body = String::new();
        if !self.stdout.is_empty() {
            body.push_str("stdout:\n");
            body.push_str(&self.stdout);
        }
        if !self.stderr.is_empty() {
            if !body.is_empty() {
                body.push_str("\n\n");
            }
            body.push_str("stderr:\n");
            body.push_str(&self.stderr);
        }
        if body.is_empty() {
            body.push_str("(no output)");
        }
        body.push_str("\n\n");
        if let Some(policy) = self.denied_by_policy {
            // A refused file effect is a policy fact, not a command
            // failure: name the boundary, then let the exit marker
            // stay the tail.
            body.push_str(&denial_marker(policy));
            body.push('\n');
        }
        if self.timed_out {
            // The timeout marker wins even when the command
            // trapped SIGTERM and exited 0 — the run was cut
            // short, and the model must know.
            body.push_str(&format!("[timed out after {timeout_secs}s]"));
        } else if let Some(signal) = self.signal {
            body.push_str(&format!("[killed by signal: {signal}]"));
        } else {
            body.push_str(&format!(
                "[exit code: {}]",
                self.exit_code.unwrap_or(-1)
            ));
        }
        body
    }
}

/// Signal an entire process group (`-pgid`), falling back to the
/// direct child when the group is already gone.
fn signal_group(pgid: i32, sig: i32) {
    if pgid > 0 && unsafe { libc::kill(-pgid, sig) } == -1 {
        // Group already reaped: best-effort direct signal.
        let _ = unsafe { libc::kill(pgid, sig) };
    }
}

/// Spawn a background task draining one pipe into a shared
/// buffer. Concurrent draining is mandatory: a full pipe buffer
/// would otherwise deadlock the child.
fn spawn_reader<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    mut pipe: R,
) -> (Arc<Mutex<Vec<u8>>>, tokio::task::JoinHandle<()>) {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buffer);
    let handle = tokio::spawn(async move {
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Ok(mut buf) = sink.lock() {
                        buf.extend_from_slice(&chunk[..n]);
                    }
                }
            }
        }
    });
    (buffer, handle)
}

/// Bounded wait for the reader tasks so a surviving grandchild
/// holding the pipe cannot stall the tool result.
async fn finish_reader(
    buffer: &Arc<Mutex<Vec<u8>>>,
    handle: tokio::task::JoinHandle<()>,
) -> String {
    let _ =
        tokio::time::timeout(Duration::from_secs(KILL_GRACE_SECS + 1), handle)
            .await;
    buffer
        .lock()
        .map(|buf| String::from_utf8_lossy(&buf).into_owned())
        .unwrap_or_default()
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        // Schema is generated from `ShellArgs` via `schemars`,
        // so the type and the LLM-facing schema cannot drift —
        // including `additionalProperties: false` and the
        // `timeout_secs` default, all declared inline via
        // `#[schemars(extend(...))]`.
        serde_json::to_value(schemars::schema_for!(ShellArgs))
            .expect("ShellArgs schema is always serializable")
    }

    fn mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    /// R124: MCP-native descriptor hints. `shell` may mutate the
    /// workspace (or the wider system under a permissive policy);
    /// the `destructive` hint is the inverse of the current
    /// `ExecutionPolicy::is_confined()` so a confined policy
    /// (e.g. `WorkspaceReadOnly`) reports the model-side
    /// restriction as part of the descriptor. No outbound network
    /// (HTTP fetch lives in `synthia-tool-web`).
    fn annotations(&self) -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            read_only_hint: Some(self.policy.is_confined()),
            destructive_hint: Some(!self.policy.is_confined()),
            idempotent_hint: Some(false),
            open_world_hint: Some(false),
        })
    }

    /// R17: shell output carries the exit-status marker
    /// (`[exit code: N]` / `[timed out after Ns]` /
    /// `[killed by signal: N]`) as its last line; the render kind
    /// tells a UI to give the block a terminal chrome.
    fn output_definition(&self) -> ToolOutputDefinition {
        ToolOutputDefinition::passthrough("shell")
            .with_kind(RenderKind::Shell)
            .with_title("Shell")
            .with_presentation(
                "exit_marker",
                serde_json::json!("[exit code: N]"),
            )
    }

    async fn call(
        &self,
        input: serde_json::Value,
        context: &Context,
    ) -> ToolOutput {
        let args: ShellArgs = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        if self.is_denied(&args.command) {
            return ToolOutput::error(format!(
                "Command denied by safety policy (matches default deny pattern): {}",
                args.command
            ));
        }

        let timeout_secs = args
            .timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS);

        // Resolve the argv before running anything: a policy the
        // host cannot enforce refuses here, so the command never
        // starts unconfined.
        let confined = match resolve_confined_command(
            self.sandbox.as_deref(),
            self.policy,
            &context.workspace_root,
        ) {
            Ok(confined) => confined,
            Err(error) => return ToolOutput::error(error.to_string()),
        };
        let dialect = denial_dialect(self.sandbox.as_deref(), self.policy);

        let run = execute(
            &args.command,
            context,
            Duration::from_secs(timeout_secs),
            &confined,
        )
        .await;

        match run {
            Ok(mut run) => {
                // A refusal by the sandbox is a policy boundary,
                // not a broken command: keep the run's data and
                // add the marker the model is told to expect.
                if classify_denial(run.exit_code, &run.stderr, dialect) {
                    run.denied_by_policy = Some(self.policy);
                }
                ToolOutput::text(run.render(timeout_secs))
            }
            Err(message) => ToolOutput::error(message),
        }
    }
}

/// Spawn and supervise one command. `Ok` carries the collected
/// run (including timeouts — rendered as markers); `Err` is
/// reserved for infrastructure failures (spawn errors).
///
/// The argv comes from `confined` rather than being built here:
/// under the default policy that is `sh -c <command>` verbatim,
/// under a confined policy it is the backend's wrap with the same
/// payload at the tail.
async fn execute(
    command: &str,
    context: &Context,
    timeout: Duration,
    confined: &ConfinedCommand,
) -> Result<ShellRun, String> {
    let argv = confined.argv_for(command);
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    // Run inside the workspace (dsh session-cwd semantics); the
    // agent loop feeds `workspace_root` on every dispatch.
    if !context.workspace_root.as_os_str().is_empty() {
        cmd.current_dir(&context.workspace_root);
    }
    // Interactive/paged output is noise for a model: suppress
    // colour, prompts, and pagers.
    cmd.env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("PAGER", "cat")
        .env("GIT_PAGER", "cat");
    // Own process group → the whole spawned tree is addressable
    // as `-pgid` when the timeout fires.
    cmd.process_group(0);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn shell: {e}"))?;
    let pgid = child.id().unwrap_or(0) as i32;

    let (stdout_buf, stdout_task) =
        spawn_reader(child.stdout.take().expect("piped stdout"));
    let (stderr_buf, stderr_task) =
        spawn_reader(child.stderr.take().expect("piped stderr"));

    // First cause wins: natural exit vs. deadline.
    let timed_out;
    let status = tokio::select! {
        biased;
        status = child.wait() => {
            timed_out = false;
            Some(status.map_err(|e| format!("wait failed: {e}"))?)
        }
        _ = tokio::time::sleep(timeout) => {
            timed_out = true;
            None
        }
    };

    if timed_out {
        // SIGTERM → grace → SIGKILL on the whole group. The
        // grace timer is NOT cleared when the leader exits: the
        // tree may outlive it.
        signal_group(pgid, libc::SIGTERM);
        let _ = tokio::time::timeout(
            Duration::from_secs(KILL_GRACE_SECS),
            child.wait(),
        )
        .await;
        signal_group(pgid, libc::SIGKILL);
        let _ = child.wait().await;
    }

    let stdout = finish_reader(&stdout_buf, stdout_task).await;
    let stderr = finish_reader(&stderr_buf, stderr_task).await;

    let (exit_code, signal) = match status {
        Some(status) => {
            let code = status.code();
            let signal = if timed_out { None } else { signal_of(&status) };
            (code, signal)
        }
        None => (None, None),
    };

    Ok(ShellRun {
        stdout,
        stderr,
        exit_code,
        signal,
        timed_out,
        denied_by_policy: None,
    })
}

#[cfg(test)]
mod tests;
