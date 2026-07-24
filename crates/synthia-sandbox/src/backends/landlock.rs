//! Linux Landlock sandbox backend.
//!
//! This module probes kernel Landlock support and, when available, builds a
//! path-based access-control ruleset for the workspace. Landlock is
//! filesystem-only sandboxing and does not provide namespace isolation; see
//! the crate README for details on how it differs from Bubblewrap.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use landlock::{
    ABI,
    Access,
    AccessFs,
    PathBeneath,
    PathFd,
    Ruleset,
    RulesetAttr,
    RulesetCreated,
    RulesetCreatedAttr,
    RulesetError,
};

use crate::{SandboxAttempt, SandboxError, SandboxManager, SandboxPolicy};

/// Linux Landlock backend.
///
/// Detects Landlock ABI support and, when available, returns
/// [`SandboxAttempt::Landlock`] for `Standard`/`Strict` policies.
#[derive(Debug, Clone)]
pub struct LandlockBackend {
    workspace: PathBuf,
}

impl LandlockBackend {
    /// Create a new Landlock backend rooted at `workspace`.
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }

    /// Return `true` if the running kernel supports Landlock.
    fn is_available() -> bool {
        Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V1))
            .and_then(|ruleset| ruleset.create())
            .is_ok()
    }
}

#[async_trait]
impl SandboxManager for LandlockBackend {
    async fn select(
        &self,
        policy: SandboxPolicy,
        _tool_type: &str,
        platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        match policy {
            SandboxPolicy::None => Ok(SandboxAttempt::None),
            SandboxPolicy::Standard | SandboxPolicy::Strict => {
                if platform != "linux" {
                    return Ok(SandboxAttempt::Unavailable);
                }
                if !Self::is_available() {
                    return Ok(SandboxAttempt::Unavailable);
                }
                Ok(SandboxAttempt::Landlock {
                    workspace: self.workspace.clone(),
                    policy: policy.clone(),
                })
            }
            SandboxPolicy::Custom(_) => Ok(SandboxAttempt::Unavailable),
        }
    }
}

/// Build a Landlock ruleset for `workspace` according to `policy`.
///
/// * `Standard` grants read/write access to `workspace` and read-only (plus
///   execute) access to a fixed set of system directories.
/// * `Strict` grants read/write access to `workspace` only.
pub(crate) fn build_ruleset(
    workspace: &Path,
    policy: SandboxPolicy,
) -> Result<RulesetCreated, SandboxError> {
    let abi = ABI::V1;

    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(ruleset_error)?
        .create()
        .map_err(ruleset_error)?;

    let workspace_fd = PathFd::new(workspace)
        .map_err(|e| SandboxError::new("INVALID_WORKSPACE", e.to_string()))?;
    ruleset = ruleset
        .add_rule(PathBeneath::new(workspace_fd, AccessFs::from_all(abi)))
        .map_err(ruleset_error)?;

    if policy == SandboxPolicy::Standard {
        let ro_access = AccessFs::from_read(abi) | AccessFs::Execute;
        for dir in ["/usr", "/bin", "/lib", "/lib64", "/sbin", "/proc", "/dev"]
        {
            let fd = PathFd::new(dir).map_err(|e| {
                SandboxError::new("INVALID_SYSTEM_DIR", format!("{dir}: {e}"))
            })?;
            ruleset = ruleset
                .add_rule(PathBeneath::new(fd, ro_access))
                .map_err(ruleset_error)?;
        }
    }

    Ok(ruleset)
}

fn ruleset_error(e: RulesetError) -> SandboxError {
    SandboxError::new("LANDLOCK_RULESET", e.to_string())
}

#[cfg(test)]
mod tests {
    use tokio::process::Command;

    use super::*;
    use crate::SandboxAttempt;

    #[tokio::test]
    async fn select_none_returns_none() {
        let backend = LandlockBackend::new(std::env::current_dir().unwrap());
        let attempt = backend
            .select(SandboxPolicy::None, "bash", "linux")
            .await
            .unwrap();
        assert!(matches!(attempt, SandboxAttempt::None));
    }

    #[tokio::test]
    async fn select_unsupported_platform_returns_unavailable() {
        let backend = LandlockBackend::new(std::env::current_dir().unwrap());
        let attempt = backend
            .select(SandboxPolicy::Standard, "bash", "windows")
            .await
            .unwrap();
        assert!(matches!(attempt, SandboxAttempt::Unavailable));
    }

    #[tokio::test]
    async fn select_custom_returns_unavailable() {
        let backend = LandlockBackend::new(std::env::current_dir().unwrap());
        let attempt = backend
            .select(
                SandboxPolicy::Custom("ro=/opt".to_string()),
                "bash",
                "linux",
            )
            .await
            .unwrap();
        assert!(matches!(attempt, SandboxAttempt::Unavailable));
    }

    #[tokio::test]
    async fn select_standard_and_strict_maps_to_landlock_when_available() {
        let backend = LandlockBackend::new(std::env::current_dir().unwrap());
        let available = LandlockBackend::is_available();

        for policy in [SandboxPolicy::Standard, SandboxPolicy::Strict] {
            let attempt = backend
                .select(policy.clone(), "bash", "linux")
                .await
                .unwrap();

            if available {
                assert!(matches!(
                    attempt,
                    SandboxAttempt::Landlock {
                        workspace: _,
                        policy: ref p,
                    } if *p == policy
                ));
            } else {
                assert!(matches!(attempt, SandboxAttempt::Unavailable));
            }
        }
    }

    #[tokio::test]
    async fn landlock_wrap_preserves_command_arguments() {
        if !LandlockBackend::is_available() {
            return;
        }

        let workspace = std::env::current_dir().unwrap();
        let attempt = SandboxAttempt::Landlock {
            workspace: workspace.clone(),
            policy: SandboxPolicy::Standard,
        };
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        attempt.wrap(&mut cmd).unwrap();

        let output = cmd.output().await.unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
    }
}
