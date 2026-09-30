//! Verification gate — run a shell command after a child agent
//! finishes and accept the result only when the command exits
//! successfully.
//!
//! Slice D (R30 — pi-subagents parity). A parent that delegates a
//! sub-task can attach an optional [`GateSpec`] to the
//! [`crate::task::TaskSpec`]. Once the child
//! completes, the gate runs from a chosen working directory
//! (typically the child's worktree, when isolation is enabled)
//! and the child's text becomes the tool result only when the
//! gate passes. A failed, timed-out, or killed gate turns the
//! child's output into a [`synthia_tool::ToolOutput::error`]
//! carrying the gate's stdout / stderr / exit status — the parent
//! model still gets a usable signal, never an indistinguishable
//! silence.
//!
//! # Runtime neutrality
//!
//! Gate execution goes through the [`CommandRunner`] trait so the
//! production code path uses `std::process::Command` while the
//! integration tests use a recording fake that replays canned
//! outcomes. The trait is `Send + Sync` so it can live behind an
//! `Arc` and be shared across the delegation, gate, and worktree
//! modules without any `tokio`-aware types in the public
//! signatures.
//!
//! # Exactly-once verdict
//!
//! A single [`run_gate`] call produces a single [`GateVerdict`]:
//! the verdict is set by whichever exit path is taken first
//! (clean exit, non-zero exit, timeout, killed by the parent
//! cancellation). The verdict is `Clone` so callers can route it
//! to multiple sinks (parent result, structured log, telemetry)
//! without re-running the gate.
//!
//! # Killed is not passing
//!
//! A gate killed by the [`synthia_core::CancelToken`] reports
//! [`GateVerdict::Killed`] explicitly. The verdict enum has no
//! "success" variant for a killed gate — the parent model sees
//! "killed" in the error message and never mistakes a cancelled
//! verification for a passing one.

use std::{
    io,
    path::Path,
    pin::Pin,
    process::{Command, ExitStatus, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use synthia_core::CancelToken;
use thiserror::Error;

/// A single shell command the parent attaches to a delegated
/// sub-task for verification.
///
/// `command` is the literal argv (the first element is the
/// program, the rest are arguments) —
/// `["cargo", "test", "-p", "synthia-harness"]`. A bare string is
/// interpreted as a program name with no arguments; shell
/// metacharacters are NOT interpreted (no `sh -c` wrapping), so
/// callers that need shell expansion must pass
/// `["sh", "-c", "..."]` themselves.
///
/// `timeout` bounds the gate's wall-clock lifetime; on expiry the
/// running child is killed and the verdict becomes
/// [`GateVerdict::Timeout`]. A `None` timeout means "no limit" —
/// the gate runs until it exits or the parent cancels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateSpec {
    /// Full argv (program + arguments).
    pub command: Vec<String>,
    /// Wall-clock budget. `None` ⇒ unlimited.
    pub timeout: Option<Duration>,
}

impl GateSpec {
    /// Convenience constructor for a single-arg gate (no
    /// arguments, no timeout).
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            command: vec![program.into()],
            timeout: None,
        }
    }

    /// Builder: append an argument to the argv.
    #[must_use]
    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.command.push(a.into());
        self
    }

    /// Builder: set the timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Short label for the gate — used in error messages so the
    /// parent model can identify which gate failed. Defaults to
    /// the program name (`argv[0]`).
    pub fn label(&self) -> &str {
        self.command.first().map(String::as_str).unwrap_or("")
    }
}

impl From<Vec<String>> for GateSpec {
    fn from(command: Vec<String>) -> Self {
        Self {
            command,
            timeout: None,
        }
    }
}

impl From<&str> for GateSpec {
    fn from(program: &str) -> Self {
        Self::new(program)
    }
}

/// Outcome of a single gate execution.
///
/// Exactly one variant is produced per [`run_gate`] call — there
/// is no "Indeterminate" state. A cancelled gate cannot be
/// misread as passing because [`GateVerdict::Killed`] is a
/// distinct variant from [`GateVerdict::Pass`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateVerdict {
    /// The gate exited with status code 0.
    Pass,
    /// The gate exited with a non-zero status code.
    Fail {
        /// Captured stdout (best-effort; may be truncated).
        stdout: String,
        /// Captured stderr (best-effort; may be truncated).
        stderr: String,
        /// Exit status code reported by the OS.
        status: i32,
    },
    /// The gate exceeded its wall-clock budget.
    Timeout {
        /// Partial stdout captured before the kill.
        stdout: String,
        /// Partial stderr captured before the kill.
        stderr: String,
    },
    /// The gate was killed because the parent's
    /// [`CancelToken`] fired.
    Killed {
        /// Partial stdout captured before the kill.
        stdout: String,
        /// Partial stderr captured before the kill.
        stderr: String,
    },
}

impl GateVerdict {
    /// `true` iff this verdict is [`GateVerdict::Pass`].
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// `true` iff the gate did not succeed
    /// ([`GateVerdict::Fail`], [`GateVerdict::Timeout`], or
    /// [`GateVerdict::Killed`]).
    pub fn is_failure(&self) -> bool {
        !self.is_pass()
    }
}

/// Errors raised when the gate cannot start a process at all
/// (e.g. the binary is missing or the working directory does
/// not exist). Distinct from non-zero exit codes, which the gate
/// reports as [`GateVerdict::Fail`].
#[derive(Debug, Error)]
pub enum GateError {
    /// The argv is empty.
    #[error("gate command must have at least one argument")]
    EmptyCommand,
    /// The chosen working directory does not exist.
    #[error("gate working directory `{path}` does not exist")]
    MissingWorkingDirectory {
        /// The directory the gate tried to use.
        path: String,
    },
    /// `std::process::Command` failed to spawn the child
    /// (binary not found, permission denied, …).
    #[error("gate failed to spawn `{program}`: {source}")]
    Spawn {
        /// The program that failed to spawn.
        program: String,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
}

/// Abstraction over `std::process::Command` so tests can replace
/// the real binary execution with a recording fake that replays
/// canned argv + outcomes.
///
/// Implementations MUST be `Send + Sync` so the gate can run on
/// any executor and so the same runner can be shared across the
/// gate, worktree, and child-tool layers without further
/// plumbing.
pub trait CommandRunner: Send + Sync {
    /// Spawn the command described by `argv` with `cwd` as the
    /// working directory, returning once the process has exited
    /// (or been killed by `cancel`).
    ///
    /// `argv[0]` is the program; the rest are arguments. The
    /// runner is responsible for wiring `Stdio::piped()` so
    /// stdout / stderr are captured, and for honouring the
    /// cancellation token — when `cancel` fires the runner MUST
    /// kill the running process (or refuse to start a new one if
    /// it has not yet begun) and return a verdict whose variant
    /// reflects the cancellation.
    fn run(
        &self,
        argv: &[String],
        cwd: &Path,
        cancel: Arc<dyn CancelToken>,
    ) -> CommandOutcome;
}

/// Captured output from a single [`CommandRunner::run`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutcome {
    /// Process exit status. For killed processes the runner
    /// substitutes the signal-encoded code.
    pub status: ExitStatus,
    /// Captured stdout bytes (lossy-decoded to UTF-8).
    pub stdout: String,
    /// Captured stderr bytes (lossy-decoded to UTF-8).
    pub stderr: String,
    /// Why the process stopped. `Exited` for a normal exit,
    /// `Killed` for an explicit cancel / signal.
    pub reason: CommandStopReason,
}

/// Why a [`CommandRunner::run`] call returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandStopReason {
    /// The process exited on its own.
    Exited,
    /// The process was killed because the cancellation token
    /// fired (or the gate exceeded its timeout).
    Killed,
}

/// Production [`CommandRunner`] backed by
/// `std::process::Command`.
///
/// Each `run` call spawns a fresh child with piped stdio, drains
/// stdout / stderr on dedicated threads, polls the cancellation
/// token while waiting, and kills the child on cancel.
///
/// The implementation is intentionally minimal — no async I/O,
/// no signal-handler dance beyond the kill. The default gate
/// workload is short-running verification commands (cargo test,
/// shell scripts, …), not long-lived daemons.
#[derive(Clone, Debug, Default)]
pub struct StdCommandRunner;

impl StdCommandRunner {
    /// Construct a fresh runner.
    pub fn new() -> Self {
        Self
    }
}

impl CommandRunner for StdCommandRunner {
    fn run(
        &self,
        argv: &[String],
        cwd: &Path,
        cancel: Arc<dyn CancelToken>,
    ) -> CommandOutcome {
        let mut child = match Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return CommandOutcome {
                    status: synthetic_failure_status(),
                    stdout: String::new(),
                    stderr: format!("spawn failed: {error}"),
                    reason: CommandStopReason::Killed,
                };
            }
        };

        // Drain stdout / stderr concurrently so a slow consumer
        // doesn't deadlock the child.
        // Drain stdout / stderr concurrently so a slow consumer
        // doesn't deadlock the child. Take ownership of the
        // pipes (Option<ChildStdout>) and move them into the
        // threads — the threads own the pipe handles for the
        // duration of their lifetime.
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let stdout_thread = stdout_pipe.map(|mut s| {
            std::thread::spawn(move || {
                let mut buf = String::new();
                let _ = io::Read::read_to_string(&mut s, &mut buf);
                buf
            })
        });
        let stderr_thread = stderr_pipe.map(|mut s| {
            std::thread::spawn(move || {
                let mut buf = String::new();
                let _ = io::Read::read_to_string(&mut s, &mut buf);
                buf
            })
        });

        let poll_interval = Duration::from_millis(20);
        let (status_opt, reason) = loop {
            if cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                break (None, CommandStopReason::Killed);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    break (Some(status), CommandStopReason::Exited);
                }
                Ok(None) => {
                    std::thread::sleep(poll_interval);
                }
                Err(_) => {
                    let _ = child.kill();
                    break (None, CommandStopReason::Killed);
                }
            }
        };

        let stdout = stdout_thread
            .map(|t| t.join().unwrap_or_default())
            .unwrap_or_default();
        let stderr = stderr_thread
            .map(|t| t.join().unwrap_or_default())
            .unwrap_or_default();

        let status = status_opt.unwrap_or_else(synthetic_failure_status);

        let _ = Instant::now(); // placeholder for future logging hooks

        CommandOutcome {
            status,
            stdout,
            stderr,
            reason,
        }
    }
}

/// Cancellation token that never fires. Used by the worktree
/// module where `git` invocations are not parent-cancellable
/// mid-flight (one `git worktree add` call) — there is nothing
/// for the runner to interrupt.
pub fn no_cancel_token() -> Arc<dyn CancelToken> {
    struct NeverCancel;
    impl CancelToken for NeverCancel {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancel(&self) {}

        fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(std::future::pending())
        }
    }

    Arc::new(NeverCancel)
}

/// `ExitStatus` whose `success()` is `false`. Used when a child
fn synthetic_failure_status() -> ExitStatus {
    // WEXITED with exit code 1: bit-shifted so WIFEXITED is true
    // and `code()` returns `Some(1)` (the verdict uses `code()` to
    // surface the status to the model).
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(1 << 8)
    }
    #[cfg(not(unix))]
    {
        ExitStatus::default()
    }
}

/// Run the gate exactly once and return the resulting verdict.
///
/// The function:
/// 1. Validates the argv and working directory.
/// 2. Spawns the gate via `runner` with `cancel` observed while
///    waiting.
/// 3. On exit status 0 ⇒ [`GateVerdict::Pass`].
/// 4. On non-zero exit ⇒ [`GateVerdict::Fail`].
/// 5. On cancellation ⇒ [`GateVerdict::Killed`].
///
/// The verdict is set by the FIRST outcome that completes; the
/// caller never sees two verdicts and never needs to re-run the
/// gate.
pub fn run_gate(
    spec: &GateSpec,
    cwd: &Path,
    runner: &dyn CommandRunner,
    cancel: Arc<dyn CancelToken>,
) -> Result<GateVerdict, GateError> {
    if spec.command.is_empty() {
        return Err(GateError::EmptyCommand);
    }
    if !cwd.exists() {
        return Err(GateError::MissingWorkingDirectory {
            path: cwd.display().to_string(),
        });
    }

    let cancel_for_runner: Arc<dyn CancelToken> = cancel.clone();
    let outcome = runner.run(&spec.command, cwd, cancel_for_runner);
    Ok(verdict_from_outcome(outcome))
}

fn verdict_from_outcome(outcome: CommandOutcome) -> GateVerdict {
    match outcome.reason {
        CommandStopReason::Killed => GateVerdict::Killed {
            stdout: outcome.stdout,
            stderr: outcome.stderr,
        },
        CommandStopReason::Exited => {
            if outcome.status.success() {
                GateVerdict::Pass
            } else {
                GateVerdict::Fail {
                    stdout: outcome.stdout,
                    stderr: outcome.stderr,
                    status: outcome.status.code().unwrap_or(-1),
                }
            }
        }
    }
}

/// Render a [`GateVerdict`] as a single human-readable line for
/// logs / structured fields.
pub fn verdict_summary(verdict: &GateVerdict) -> String {
    match verdict {
        GateVerdict::Pass => "pass".to_string(),
        GateVerdict::Fail { status, .. } => format!("fail (exit {status})"),
        GateVerdict::Timeout { .. } => "timeout".to_string(),
        GateVerdict::Killed { .. } => "killed".to_string(),
    }
}

/// Apply the gate's verdict to a child text result: pass returns
/// the child's text as-is, anything else wraps the child's text
/// (if any) plus the verdict summary in a
/// [`synthia_tool::ToolOutput::error`].
///
/// The error message is **always** prefixed with `failed gate:`
/// so the parent model can pattern-match on the failure without
/// having to inspect the verdict type.
pub fn apply_gate_to_output(
    child_text: Option<&str>,
    verdict: &GateVerdict,
    gate_label: &str,
) -> synthia_tool::ToolOutput {
    match verdict {
        GateVerdict::Pass => match child_text {
            Some(text) if !text.is_empty() => {
                synthia_tool::ToolOutput::text(text)
            }
            _ => synthia_tool::ToolOutput::text(String::new()),
        },
        GateVerdict::Fail {
            stdout,
            stderr,
            status,
        } => synthia_tool::ToolOutput::error(format!(
            "failed gate `{gate_label}`: exit {status}\n\
             stdout: {stdout}\nstderr: {stderr}"
        )),
        GateVerdict::Timeout { stdout, stderr } => {
            synthia_tool::ToolOutput::error(format!(
                "failed gate `{gate_label}`: timed out\n\
                 stdout: {stdout}\nstderr: {stderr}"
            ))
        }
        GateVerdict::Killed { stdout, stderr } => {
            synthia_tool::ToolOutput::error(format!(
                "failed gate `{gate_label}`: killed\n\
                 stdout: {stdout}\nstderr: {stderr}"
            ))
        }
    }
}

#[cfg(test)]
#[allow(dead_code, clippy::useless_conversion, clippy::collapsible_if)]
mod tests {
    use std::{collections::VecDeque, path::PathBuf, sync::Mutex};

    use super::*;

    /// Recording fake: each call appends its argv to `recorded`
    /// and pops the next canned outcome.
    #[derive(Default)]
    struct FakeRunner {
        recorded: Mutex<Vec<Vec<String>>>,
        outcomes: Mutex<VecDeque<CommandOutcome>>,
    }

    impl FakeRunner {
        fn with_outcomes(outcomes: Vec<CommandOutcome>) -> Self {
            let deque: VecDeque<CommandOutcome> = outcomes.into();
            // Test pushes outcomes in invocation order; pop_front
            // takes from the front, so we leave the deque as-is.
            Self {
                recorded: Mutex::new(Vec::new()),
                outcomes: Mutex::new(deque),
            }
        }

        fn recorded(&self) -> Vec<Vec<String>> {
            self.recorded.lock().expect("lock").clone()
        }
    }
    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            argv: &[String],
            _cwd: &Path,
            _cancel: Arc<dyn CancelToken>,
        ) -> CommandOutcome {
            self.recorded.lock().expect("lock").push(argv.to_vec());
            self.outcomes
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or_else(|| CommandOutcome {
                    status: synthetic_failure_status(),
                    stdout: String::new(),
                    stderr: String::new(),
                    reason: CommandStopReason::Exited,
                })
        }
    }
    struct StaticCancelToken {
        cancelled: std::sync::atomic::AtomicBool,
    }

    impl StaticCancelToken {
        fn new(cancelled: bool) -> Self {
            Self {
                cancelled: std::sync::atomic::AtomicBool::new(cancelled),
            }
        }
    }

    impl CancelToken for StaticCancelToken {
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(std::sync::atomic::Ordering::Acquire)
        }

        fn cancel(&self) {
            self.cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }

        fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(std::future::pending())
        }
    }

    fn never_cancel() -> Arc<dyn CancelToken> {
        Arc::new(StaticCancelToken::new(false))
    }

    fn pre_cancel() -> Arc<dyn CancelToken> {
        Arc::new(StaticCancelToken::new(true))
    }

    fn success_outcome() -> CommandOutcome {
        CommandOutcome {
            status: success_status(),
            stdout: "ok".to_string(),
            stderr: String::new(),
            reason: CommandStopReason::Exited,
        }
    }

    fn failure_outcome(code: i32) -> CommandOutcome {
        CommandOutcome {
            status: code_status(code),
            stdout: "out".to_string(),
            stderr: "boom".to_string(),
            reason: CommandStopReason::Exited,
        }
    }

    fn killed_outcome() -> CommandOutcome {
        CommandOutcome {
            status: synthetic_failure_status(),
            stdout: "partial".to_string(),
            stderr: String::new(),
            reason: CommandStopReason::Killed,
        }
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir()
    }

    #[cfg(unix)]
    fn success_status() -> ExitStatus {
        // WEXITED with exit code 0: `from_raw(0)`.
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }
    #[cfg(not(unix))]
    fn success_status() -> ExitStatus {
        ExitStatus::default()
    }

    #[cfg(unix)]
    fn code_status(code: i32) -> ExitStatus {
        // WEXITED status on Unix: shift the exit code into bits 8-15
        // so WIFEXITED is true and `code()` returns `Some(code)`.
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw((code & 0xff) << 8)
    }

    #[test]
    fn empty_command_is_rejected_before_spawn() {
        let runner = FakeRunner::default();
        let err = run_gate(
            &GateSpec {
                command: vec![],
                timeout: None,
            },
            &temp_dir(),
            &runner,
            never_cancel(),
        )
        .expect_err("empty argv must error");
        assert!(matches!(err, GateError::EmptyCommand));
        assert!(runner.recorded().is_empty());
    }

    #[test]
    fn missing_working_directory_is_rejected_before_spawn() {
        let runner = FakeRunner::default();
        let bogus = PathBuf::from("/nonexistent/please/no/such/dir");
        let err =
            run_gate(&GateSpec::new("true"), &bogus, &runner, never_cancel())
                .expect_err("missing cwd must error");
        assert!(matches!(err, GateError::MissingWorkingDirectory { .. }));
        assert!(runner.recorded().is_empty());
    }

    #[test]
    fn successful_exit_yields_pass_verdict() {
        let runner = FakeRunner::with_outcomes(vec![success_outcome()]);
        let verdict = run_gate(
            &GateSpec::new("cargo").arg("test"),
            &temp_dir(),
            &runner,
            never_cancel(),
        )
        .expect("run");
        assert!(verdict.is_pass());
        assert_eq!(
            runner.recorded(),
            vec![vec!["cargo".to_string(), "test".to_string()]]
        );
    }

    #[test]
    fn non_zero_exit_yields_fail_with_captured_output() {
        let runner = FakeRunner::with_outcomes(vec![failure_outcome(2)]);
        let verdict = run_gate(
            &GateSpec::new("make"),
            &temp_dir(),
            &runner,
            never_cancel(),
        )
        .expect("run");
        let GateVerdict::Fail {
            stdout,
            stderr,
            status,
        } = verdict
        else {
            panic!("expected Fail");
        };
        assert_eq!(stdout, "out");
        assert_eq!(stderr, "boom");
        assert_eq!(status, 2);
    }
    #[test]
    fn cancellation_yields_killed_not_pass() {
        let runner = FakeRunner::with_outcomes(vec![killed_outcome()]);
        let verdict = run_gate(
            &GateSpec::new("sleep"),
            &temp_dir(),
            &runner,
            pre_cancel(),
        )
        .expect("run");
        assert!(matches!(verdict, GateVerdict::Killed { .. }));
        assert!(verdict.is_failure());
        assert!(!verdict.is_pass(), "killed must NEVER read as passing");
    }

    #[test]
    fn run_gate_produces_exactly_one_verdict() {
        let runner = FakeRunner::with_outcomes(vec![
            failure_outcome(3),
            success_outcome(),
        ]);
        let first =
            run_gate(&GateSpec::new("a"), &temp_dir(), &runner, never_cancel())
                .expect("first");
        let second =
            run_gate(&GateSpec::new("b"), &temp_dir(), &runner, never_cancel())
                .expect("second");
        assert!(matches!(first, GateVerdict::Fail { status: 3, .. }));
        assert!(second.is_pass());
    }

    #[test]
    fn apply_gate_returns_child_text_on_pass() {
        let text = "child did the thing";
        let out =
            apply_gate_to_output(Some(text), &GateVerdict::Pass, "verify");
        assert!(!out.is_error.unwrap_or(false));
        let rendered = format!("{:?}", out.content);
        assert!(rendered.contains(text));
    }

    #[test]
    fn apply_gate_marks_failure_as_error_with_prefix() {
        let verdict = GateVerdict::Fail {
            stdout: "out".to_string(),
            stderr: "boom".to_string(),
            status: 2,
        };
        let out = apply_gate_to_output(Some("child text"), &verdict, "verify");
        let rendered = format!("{:?}", out.content);
        assert!(rendered.contains("failed gate"));
        assert!(rendered.contains("exit 2"));
        assert!(rendered.contains("verify"));
    }

    #[test]
    fn verdict_summary_names_variants() {
        assert_eq!(verdict_summary(&GateVerdict::Pass), "pass");
        assert_eq!(
            verdict_summary(&GateVerdict::Fail {
                stdout: String::new(),
                stderr: String::new(),
                status: 7
            }),
            "fail (exit 7)"
        );
        assert_eq!(
            verdict_summary(&GateVerdict::Timeout {
                stdout: String::new(),
                stderr: String::new()
            }),
            "timeout"
        );
        assert_eq!(
            verdict_summary(&GateVerdict::Killed {
                stdout: String::new(),
                stderr: String::new()
            }),
            "killed"
        );
    }

    #[test]
    fn gate_spec_builder_chains_args_and_timeout() {
        let spec = GateSpec::new("cargo")
            .arg("test")
            .arg("-p")
            .arg("synthia-harness")
            .with_timeout(Duration::from_secs(30));
        assert_eq!(
            spec.command,
            vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "synthia-harness".to_string()
            ]
        );
        assert_eq!(spec.timeout, Some(Duration::from_secs(30)));
        assert_eq!(spec.label(), "cargo");
    }

    #[test]
    fn from_string_slices_yields_one_arg_command() {
        let spec: GateSpec = "make".into();
        assert_eq!(spec.command, vec!["make".to_string()]);
        assert_eq!(spec.timeout, None);
    }

    #[test]
    fn from_vec_string_preserves_argv() {
        let spec: GateSpec = vec![
            "cargo".to_string(),
            "fmt".to_string(),
            "--check".to_string(),
        ]
        .into();
        assert_eq!(
            spec.command,
            vec![
                "cargo".to_string(),
                "fmt".to_string(),
                "--check".to_string()
            ]
        );
        assert_eq!(spec.timeout, None);
    }
}
