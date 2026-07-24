//! Composite sandbox manager with prioritized backend fallback.
//!
//! [`CompositeSandboxManager`] chains multiple [`SandboxManager`] backends and
//! returns the first attempt that is not [`SandboxAttempt::Unavailable`]. The
//! default Linux chain prefers Bubblewrap and falls back to Landlock when the
//! `landlock` feature is enabled.

use std::{fmt, path::PathBuf, sync::Arc};

use async_trait::async_trait;

#[cfg(feature = "landlock")]
use crate::backends::landlock::LandlockBackend;
use crate::{
    SandboxAttempt,
    SandboxError,
    SandboxManager,
    SandboxPolicy,
    backends::bubblewrap::BubblewrapBackend,
};

/// A sandbox manager that selects the first usable backend from an ordered chain.
///
/// The composite manager queries backends in the order they were added and returns
/// the first [`SandboxAttempt`] that is not [`SandboxAttempt::Unavailable`]. If every
/// backend reports `Unavailable`, it returns `Unavailable` and never silently
/// downgrades to [`SandboxAttempt::None`].
///
/// [`SandboxPolicy::None`] is short-circuited to [`SandboxAttempt::None`] without
/// querying any backend.
#[derive(Clone, Default)]
pub struct CompositeSandboxManager {
    backends: Vec<Arc<dyn SandboxManager>>,
}

impl fmt::Debug for CompositeSandboxManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompositeSandboxManager")
            .field("backends", &self.backends.len())
            .finish()
    }
}

impl CompositeSandboxManager {
    /// Create an empty composite manager.
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Add a backend to the end of the chain.
    pub fn push(&mut self, backend: Arc<dyn SandboxManager>) {
        self.backends.push(backend);
    }

    /// Build the default Linux fallback chain.
    ///
    /// The chain always contains [`BubblewrapBackend`]. [`LandlockBackend`] is
    /// appended only when the `landlock` feature is enabled.
    pub fn default_linux(workspace: impl Into<PathBuf>) -> Self {
        let workspace = workspace.into();
        let mut manager = Self::new();
        manager.push(Arc::new(BubblewrapBackend::new(workspace.clone())));
        #[cfg(feature = "landlock")]
        manager.push(Arc::new(LandlockBackend::new(workspace)));
        manager
    }
}

#[async_trait]
impl SandboxManager for CompositeSandboxManager {
    async fn select(
        &self,
        policy: SandboxPolicy,
        tool_type: &str,
        platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        if policy == SandboxPolicy::None {
            return Ok(SandboxAttempt::None);
        }

        for backend in &self.backends {
            let attempt =
                backend.select(policy.clone(), tool_type, platform).await?;
            if !matches!(attempt, SandboxAttempt::Unavailable) {
                return Ok(attempt);
            }
        }

        Ok(SandboxAttempt::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use async_trait::async_trait;

    use super::*;

    struct FixedAttempt(SandboxAttempt);

    #[async_trait]
    impl SandboxManager for FixedAttempt {
        async fn select(
            &self,
            _policy: SandboxPolicy,
            _tool_type: &str,
            _platform: &str,
        ) -> Result<SandboxAttempt, SandboxError> {
            Ok(self.0.clone())
        }
    }

    struct PanickingBackend;

    #[async_trait]
    impl SandboxManager for PanickingBackend {
        async fn select(
            &self,
            _policy: SandboxPolicy,
            _tool_type: &str,
            _platform: &str,
        ) -> Result<SandboxAttempt, SandboxError> {
            panic!("should not be called for None policy")
        }
    }

    #[tokio::test]
    async fn composite_prefers_first_available() {
        let mut manager = CompositeSandboxManager::new();
        manager.push(Arc::new(FixedAttempt(SandboxAttempt::Bubblewrap {
            workspace: PathBuf::from("/tmp"),
            args: vec![],
        })));
        manager.push(Arc::new(FixedAttempt(SandboxAttempt::Landlock {
            workspace: PathBuf::from("/tmp"),
            policy: SandboxPolicy::Standard,
        })));

        let attempt = manager
            .select(SandboxPolicy::Standard, "bash", "linux")
            .await
            .unwrap();

        assert!(matches!(attempt, SandboxAttempt::Bubblewrap { .. }));
    }

    #[tokio::test]
    async fn composite_fallback_to_landlock() {
        let mut manager = CompositeSandboxManager::new();
        manager.push(Arc::new(FixedAttempt(SandboxAttempt::Unavailable)));
        manager.push(Arc::new(FixedAttempt(SandboxAttempt::Landlock {
            workspace: PathBuf::from("/tmp"),
            policy: SandboxPolicy::Standard,
        })));

        let attempt = manager
            .select(SandboxPolicy::Standard, "bash", "linux")
            .await
            .unwrap();

        assert!(matches!(attempt, SandboxAttempt::Landlock { .. }));
    }

    #[tokio::test]
    async fn composite_returns_unavailable_when_all_fail() {
        let mut manager = CompositeSandboxManager::new();
        manager.push(Arc::new(FixedAttempt(SandboxAttempt::Unavailable)));
        manager.push(Arc::new(FixedAttempt(SandboxAttempt::Unavailable)));

        let attempt = manager
            .select(SandboxPolicy::Standard, "bash", "linux")
            .await
            .unwrap();

        assert!(matches!(attempt, SandboxAttempt::Unavailable));
    }

    #[tokio::test]
    async fn composite_none_policy_short_circuits() {
        let mut manager = CompositeSandboxManager::new();
        manager.push(Arc::new(PanickingBackend));

        let attempt = manager
            .select(SandboxPolicy::None, "bash", "linux")
            .await
            .unwrap();

        assert!(matches!(attempt, SandboxAttempt::None));
    }

    #[tokio::test]
    async fn composite_empty_chain_returns_unavailable() {
        let manager = CompositeSandboxManager::new();

        let attempt = manager
            .select(SandboxPolicy::Standard, "bash", "linux")
            .await
            .unwrap();

        assert!(matches!(attempt, SandboxAttempt::Unavailable));
    }
}
