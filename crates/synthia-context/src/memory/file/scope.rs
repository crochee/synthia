//! The three memory scopes and where each one lives.

use std::path::{Path, PathBuf};

/// Where a memory directory lives.
///
/// The three scopes mirror the reference implementation: `local`
/// is the checkout-private overlay a developer can throw away,
/// `project` is shared with the repository, and `user` is shared
/// across every project on the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemoryScope {
    /// `<root>/.synthia/memory-local/<agent>` — nearest scope,
    /// never shared, never committed.
    Local,
    /// `<root>/.synthia/memory/<agent>` — shared with the
    /// repository.
    Project,
    /// `<home>/.synthia/memory/<agent>` — shared across every
    /// project on the machine.
    User,
}

impl MemoryScope {
    /// Scope precedence, nearest first: `Local`, then `Project`,
    /// then `User`. Earlier scopes shadow later ones on id
    /// collisions, and writes land in the first configured scope.
    pub const ALL: [MemoryScope; 3] =
        [MemoryScope::Local, MemoryScope::Project, MemoryScope::User];

    /// Stable scope label (`"local"` / `"project"` / `"user"`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryScope::Local => "local",
            MemoryScope::Project => "project",
            MemoryScope::User => "user",
        }
    }

    /// Parse a scope label; unknown labels yield `None`.
    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        MemoryScope::ALL
            .iter()
            .copied()
            .find(|scope| scope.as_str() == label)
    }

    /// Directory this scope resolves to for `agent`.
    ///
    /// `project_root` anchors [`Local`](MemoryScope::Local) and
    /// [`Project`](MemoryScope::Project); `home` anchors
    /// [`User`](MemoryScope::User).
    #[must_use]
    pub fn dir(
        &self,
        project_root: &Path,
        home: &Path,
        agent: &str,
    ) -> PathBuf {
        self.base(project_root, home).join(self.relative(agent))
    }

    /// Trust anchor this scope resolves against: the project root
    /// for the checkout scopes, the home directory for the user
    /// scope.
    #[must_use]
    pub fn base<'a>(&self, project_root: &'a Path, home: &'a Path) -> &'a Path {
        match self {
            MemoryScope::Local | MemoryScope::Project => project_root,
            MemoryScope::User => home,
        }
    }

    /// Path segment between the trust anchor and the agent name.
    pub(super) fn relative(&self, agent: &str) -> PathBuf {
        let segment = match self {
            MemoryScope::Local => ".synthia/memory-local",
            MemoryScope::Project | MemoryScope::User => ".synthia/memory",
        };
        Path::new(segment).join(agent)
    }
}
