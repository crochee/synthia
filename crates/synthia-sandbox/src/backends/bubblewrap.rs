use std::path::PathBuf;

use async_trait::async_trait;

use crate::{SandboxAttempt, SandboxError, SandboxManager, SandboxPolicy};

/// Linux bubblewrap backend.
///
/// Detects whether `bwrap` is present in `PATH`. When it is unavailable,
/// [`SandboxManager::select`] returns [`SandboxAttempt::Unavailable`].
#[derive(Debug, Clone)]
pub struct BubblewrapBackend {
    workspace: PathBuf,
}

impl BubblewrapBackend {
    /// Create a new bubblewrap backend rooted at `workspace`.
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }

    /// Return `true` if `bwrap` can be invoked successfully.
    async fn is_available() -> bool {
        match tokio::process::Command::new("bwrap")
            .arg("--version")
            .output()
            .await
        {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

#[async_trait]
impl SandboxManager for BubblewrapBackend {
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
                if !Self::is_available().await {
                    return Ok(SandboxAttempt::Unavailable);
                }
                Ok(SandboxAttempt::Bubblewrap {
                    workspace: self.workspace.clone(),
                    args: vec![],
                })
            }
            SandboxPolicy::Custom(_) => Ok(SandboxAttempt::Unavailable),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn select_returns_bubblewrap_or_unavailable() {
        let backend = BubblewrapBackend::new(std::env::current_dir().unwrap());
        let attempt = backend
            .select(SandboxPolicy::Standard, "bash", "linux")
            .await
            .unwrap();

        let bwrap_available = tokio::process::Command::new("bwrap")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        if bwrap_available {
            assert!(matches!(attempt, SandboxAttempt::Bubblewrap { .. }));
        } else {
            assert!(matches!(attempt, SandboxAttempt::Unavailable));
        }
    }

    #[tokio::test]
    async fn select_none_policy_returns_none() {
        let backend = BubblewrapBackend::new(std::env::current_dir().unwrap());
        let attempt = backend
            .select(SandboxPolicy::None, "bash", "linux")
            .await
            .unwrap();
        assert!(matches!(attempt, SandboxAttempt::None));
    }

    #[tokio::test]
    async fn select_unsupported_platform_returns_unavailable() {
        let backend = BubblewrapBackend::new(std::env::current_dir().unwrap());
        let attempt = backend
            .select(SandboxPolicy::Standard, "bash", "windows")
            .await
            .unwrap();
        assert!(matches!(attempt, SandboxAttempt::Unavailable));
    }
}
