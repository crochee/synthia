//! Configuration for the file-backed tier, plus the resolved
//! per-scope directories the store and the scanner share.

use std::path::{Path, PathBuf};

use super::{error::FileMemoryError, scope::MemoryScope};

/// Configuration for a [`FileMemory`](super::FileMemory) tier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMemoryConfig {
    /// Workspace root anchoring the `project` and `local` scopes.
    pub root: PathBuf,
    /// Agent name; must match `[A-Za-z0-9_-]` (at most 128
    /// characters).
    pub agent: String,
    /// Scopes searched in precedence order. `scopes[0]` receives
    /// new entries; later scopes act as read-only overlays.
    pub scopes: Vec<MemoryScope>,
    /// Read-only mode: recall works, writes are refused.
    pub read_only: bool,
    /// Base directory for the `user` scope (the home directory);
    /// defaults to `$HOME`.
    pub user_base: Option<PathBuf>,
}

impl FileMemoryConfig {
    /// Configuration searching every scope
    /// ([`MemoryScope::ALL`]), writing to the nearest one.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, agent: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            agent: agent.into(),
            scopes: MemoryScope::ALL.to_vec(),
            read_only: false,
            user_base: None,
        }
    }

    /// Replace the searched scopes (precedence order).
    #[must_use]
    pub fn with_scopes(mut self, scopes: Vec<MemoryScope>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Refuse writes; recall keeps working.
    #[must_use]
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Anchor the `user` scope at `base` instead of `$HOME`.
    #[must_use]
    pub fn with_user_base(mut self, base: impl Into<PathBuf>) -> Self {
        self.user_base = Some(base.into());
        self
    }

    /// Home directory the `user` scope resolves against.
    pub(super) fn home(&self) -> Result<PathBuf, FileMemoryError> {
        if let Some(base) = &self.user_base {
            return Ok(base.clone());
        }
        match std::env::var_os("HOME") {
            Some(home) => Ok(PathBuf::from(home)),
            None => Err(FileMemoryError::MissingHome),
        }
    }
}

/// A scope resolved against its trust anchor.
pub(super) struct ResolvedScope {
    pub(super) scope: MemoryScope,
    /// Caller-supplied anchor every component below is checked
    /// against.
    pub(super) base: PathBuf,
    /// `<segment>/<agent>`, relative to `base`.
    pub(super) relative: PathBuf,
    /// `base.join(relative)`, the scope's memory directory.
    pub(super) dir: PathBuf,
}

impl ResolvedScope {
    pub(super) fn resolve(
        scope: MemoryScope,
        config: &FileMemoryConfig,
        home: &Path,
    ) -> Self {
        let base = scope.base(&config.root, home).to_path_buf();
        let relative = scope.relative(&config.agent);
        let dir = base.join(&relative);
        Self {
            scope,
            base,
            relative,
            dir,
        }
    }
}
