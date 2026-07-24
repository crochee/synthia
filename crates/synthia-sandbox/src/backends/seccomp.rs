use async_trait::async_trait;

use crate::{SandboxAttempt, SandboxError, SandboxManager, SandboxPolicy};

/// seccomp-bpf backend stub.
///
/// The feature gate and interface are in place, but selection always reports
/// the backend as unavailable until the real implementation lands.
#[derive(Debug, Default, Clone)]
pub struct SeccompBackend;

impl SeccompBackend {
    /// Create a new seccomp backend stub.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SandboxManager for SeccompBackend {
    async fn select(
        &self,
        _policy: SandboxPolicy,
        _tool_type: &str,
        _platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        Ok(SandboxAttempt::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn select_returns_unavailable() {
        let backend = SeccompBackend::new();
        for policy in [
            SandboxPolicy::None,
            SandboxPolicy::Standard,
            SandboxPolicy::Strict,
            SandboxPolicy::Custom("rule".to_string()),
        ] {
            let attempt =
                backend.select(policy, "bash", "linux").await.unwrap();
            assert!(
                matches!(attempt, SandboxAttempt::Unavailable),
                "seccomp stub should always report unavailable, got {:?}",
                attempt
            );
        }
    }

    #[tokio::test]
    async fn select_is_independent_of_platform() {
        let backend = SeccompBackend::new();
        for platform in ["linux", "windows", "darwin"] {
            let attempt = backend
                .select(SandboxPolicy::Standard, "bash", platform)
                .await
                .unwrap();
            assert!(matches!(attempt, SandboxAttempt::Unavailable));
        }
    }
}
