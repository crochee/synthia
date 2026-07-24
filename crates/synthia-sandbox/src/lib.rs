use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

pub mod backends;
pub mod composite;

pub use composite::CompositeSandboxManager;

/// Action to take when a requested sandbox backend is unavailable.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum OnUnavailable {
    /// Fail the tool invocation.
    #[default]
    Deny,
    /// Prompt the user to explicitly approve unsandboxed execution.
    Prompt,
}

/// Security policy applied to a sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxPolicy {
    /// No sandboxing.
    None,
    /// Standard sandboxing appropriate for most tools.
    Standard,
    /// Strict sandboxing with minimal privileges.
    Strict,
    /// Custom sandbox configuration string.
    Custom(String),
}

impl SandboxPolicy {
    /// Return the configured behavior when the backend is unavailable.
    pub fn on_unavailable(&self) -> OnUnavailable {
        match self {
            SandboxPolicy::None => OnUnavailable::Prompt,
            _ => OnUnavailable::Deny,
        }
    }
}

/// A concrete sandboxing attempt selected by a [`SandboxManager`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxAttempt {
    /// No sandboxing is required.
    None,
    /// Use Linux bubblewrap.
    Bubblewrap {
        workspace: PathBuf,
        args: Vec<String>,
    },
    /// Use Linux Landlock.
    Landlock {
        workspace: PathBuf,
        policy: SandboxPolicy,
    },
    /// Use seccomp-bpf (stub).
    Seccomp { workspace: PathBuf },
    /// Sandboxing is requested but unavailable on this platform.
    Unavailable,
}

impl SandboxAttempt {
    /// Modify `command` so it runs inside the selected sandbox.
    ///
    /// For `Bubblewrap`, this rewrites the command to invoke `bwrap` with the
    /// original program and arguments appended after `--`.
    ///
    /// Note: custom stdio configuration on the original command is currently
    /// not preserved across wrapping. Configure stdio after calling `wrap`.
    pub fn wrap(&self, command: &mut Command) -> Result<(), SandboxError> {
        match self {
            SandboxAttempt::None => Ok(()),
            SandboxAttempt::Bubblewrap { workspace, args } => {
                wrap_with_bubblewrap(command, workspace, args)
            }
            SandboxAttempt::Landlock { workspace, policy } => {
                #[cfg(feature = "landlock")]
                {
                    wrap_with_landlock(command, workspace, policy)
                }
                #[cfg(not(feature = "landlock"))]
                {
                    let _ = (workspace, policy);
                    Err(SandboxError::new(
                        "UNSUPPORTED",
                        "landlock feature not enabled",
                    ))
                }
            }
            SandboxAttempt::Seccomp { .. } => Err(SandboxError::new(
                "UNSUPPORTED",
                "seccomp wrapping not implemented",
            )),
            SandboxAttempt::Unavailable => {
                Err(SandboxError::new("UNAVAILABLE", "sandbox unavailable"))
            }
        }
    }
}

fn wrap_with_bubblewrap(
    command: &mut Command,
    workspace: &PathBuf,
    extra_args: &[String],
) -> Result<(), SandboxError> {
    let original_program = command.as_std().get_program().to_os_string();
    let original_args: Vec<std::ffi::OsString> = command
        .as_std()
        .get_args()
        .map(std::ffi::OsString::from)
        .collect();
    let current_dir = command.as_std().get_current_dir().map(PathBuf::from);
    let envs: Vec<_> = command
        .as_std()
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.map(std::ffi::OsString::from)))
        .collect();

    let mut wrapped = Command::new("bwrap");
    wrapped.arg("--die-with-parent");
    wrapped.arg("--unshare-all");
    wrapped.arg("--bind");
    wrapped.arg(workspace);
    wrapped.arg("/workspace");
    wrapped.arg("--chdir");
    wrapped.arg("/workspace");
    wrapped.arg("--ro-bind");
    wrapped.arg("/usr");
    wrapped.arg("/usr");
    wrapped.arg("--ro-bind");
    wrapped.arg("/bin");
    wrapped.arg("/bin");
    wrapped.arg("--ro-bind");
    wrapped.arg("/lib");
    wrapped.arg("/lib");
    wrapped.arg("--ro-bind");
    wrapped.arg("/lib64");
    wrapped.arg("/lib64");
    wrapped.arg("--ro-bind");
    wrapped.arg("/sbin");
    wrapped.arg("/sbin");
    wrapped.arg("--proc");
    wrapped.arg("/proc");
    wrapped.arg("--dev");
    wrapped.arg("/dev");
    for arg in extra_args {
        wrapped.arg(arg);
    }
    wrapped.arg("--");
    wrapped.arg(&original_program);
    wrapped.args(&original_args);

    if let Some(dir) = current_dir {
        wrapped.current_dir(dir);
    }
    for (key, val) in envs {
        match val {
            Some(v) => {
                wrapped.env(key, v);
            }
            None => {
                wrapped.env_remove(key);
            }
        }
    }

    let _old = std::mem::replace(command, wrapped);
    Ok(())
}

#[cfg(feature = "landlock")]
/// Rewrite `command` so it applies a Landlock ruleset in the child process
/// before `exec`.
///
/// The original program, arguments, current directory, and environment are
/// preserved. The Landlock ruleset is built from `workspace` and `policy` and
/// is installed via `pre_exec`.
fn wrap_with_landlock(
    command: &mut Command,
    workspace: &std::path::Path,
    policy: &SandboxPolicy,
) -> Result<(), SandboxError> {
    use std::{
        ffi::OsString,
        os::unix::process::CommandExt,
        process::Command as StdCommand,
    };

    let original_program = command.as_std().get_program().to_os_string();
    let original_args: Vec<OsString> =
        command.as_std().get_args().map(OsString::from).collect();
    let current_dir = command.as_std().get_current_dir().map(PathBuf::from);
    let envs: Vec<(OsString, Option<OsString>)> = command
        .as_std()
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.map(OsString::from)))
        .collect();

    let mut wrapped = StdCommand::new(&original_program);
    wrapped.args(&original_args);
    if let Some(dir) = current_dir {
        wrapped.current_dir(dir);
    }
    for (key, val) in envs {
        match val {
            Some(v) => {
                wrapped.env(key, v);
            }
            None => {
                wrapped.env_remove(key);
            }
        }
    }

    let workspace = workspace.to_path_buf();
    let policy = policy.clone();
    unsafe {
        wrapped.pre_exec(
            move || match crate::backends::landlock::build_ruleset(
                &workspace,
                policy.clone(),
            ) {
                Ok(ruleset) => match ruleset.restrict_self() {
                    Ok(_) => Ok(()),
                    Err(e) => Err(std::io::Error::other(format!(
                        "landlock restrict_self failed: {e}"
                    ))),
                },
                Err(e) => Err(std::io::Error::other(format!(
                    "landlock ruleset build failed: {e}"
                ))),
            },
        );
    }

    *command = Command::from(wrapped);
    Ok(())
}

/// Structured error returned by a [`SandboxManager`] or [`SandboxAttempt::wrap`].
#[derive(Debug, thiserror::Error, Clone, Serialize, Deserialize)]
#[error("{code}: {message}")]
pub struct SandboxError {
    pub code: String,
    pub message: String,
}

impl SandboxError {
    /// Create a new sandbox error.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Selects and configures a sandboxing backend.
#[async_trait]
pub trait SandboxManager: Send + Sync {
    /// Select the best available sandbox for the given policy.
    async fn select(
        &self,
        policy: SandboxPolicy,
        tool_type: &str,
        platform: &str,
    ) -> Result<SandboxAttempt, SandboxError>;
}

/// A sandbox manager that always returns [`SandboxAttempt::None`].
///
/// Useful for tests, non-Linux platforms, or environments where the caller
/// explicitly disables OS-level sandboxing.
#[derive(Debug, Clone, Default)]
pub struct NoopSandboxManager;

#[async_trait]
impl SandboxManager for NoopSandboxManager {
    async fn select(
        &self,
        _policy: SandboxPolicy,
        _tool_type: &str,
        _platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        Ok(SandboxAttempt::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_policy_serializes_and_deserializes() {
        let policies = vec![
            SandboxPolicy::None,
            SandboxPolicy::Standard,
            SandboxPolicy::Strict,
            SandboxPolicy::Custom("extra-ro-bind=/opt".to_string()),
        ];

        for policy in policies {
            let serialized = serde_json::to_string(&policy).expect("serialize");
            let deserialized: SandboxPolicy =
                serde_json::from_str(&serialized).expect("deserialize");
            assert_eq!(policy, deserialized);
        }
    }

    #[test]
    fn sandbox_attempt_none_wrap_is_no_op() {
        let attempt = SandboxAttempt::None;
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        attempt.wrap(&mut cmd).unwrap();
        assert_eq!(cmd.as_std().get_program(), "echo");
        let args: Vec<_> = cmd.as_std().get_args().collect();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "hello");
    }

    #[test]
    fn sandbox_attempt_unavailable_wrap_fails() {
        let attempt = SandboxAttempt::Unavailable;
        let mut cmd = Command::new("echo");
        let err = attempt.wrap(&mut cmd).unwrap_err();
        assert_eq!(err.code, "UNAVAILABLE");
    }

    #[test]
    fn bubblewrap_wrap_rewrites_command() {
        let workspace = PathBuf::from("/tmp/workspace");
        let attempt = SandboxAttempt::Bubblewrap {
            workspace: workspace.clone(),
            args: vec![],
        };
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        attempt.wrap(&mut cmd).unwrap();

        assert_eq!(cmd.as_std().get_program(), "bwrap");
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(args.contains(&"--die-with-parent".to_string()));
        assert!(args.contains(&"--unshare-all".to_string()));
        assert!(args.contains(&"--bind".to_string()));
        assert!(args.contains(&workspace.to_string_lossy().to_string()));
        assert!(args.contains(&"/workspace".to_string()));
        assert!(args.contains(&"--".to_string()));
        assert!(args.contains(&"echo".to_string()));
        assert!(args.contains(&"hello".to_string()));
    }
}
