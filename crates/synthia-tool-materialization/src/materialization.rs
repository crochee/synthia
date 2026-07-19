//! `Materialization` struct (PR-5.2).
//!
//! Returned by `ScopedToolRegistry::materialize()`, this struct
//! carries the identity, visibility, provenance, and scope
//! information for a materialized tool instance. The existing
//! LIFO + RAII semantics of `ScopedToolRegistry` are preserved —
//! this struct only *adds* identity fields.

use std::sync::Arc;

use crate::{
    id::{ProviderId, ToolId},
    provenance::ToolProvenance,
    visibility::ToolVisibility,
};

/// A materialized tool instance with full identity.
///
/// Created by `ScopedToolRegistry::materialize()`, this struct
/// carries all the metadata needed to identify, audit, and
/// manage the lifecycle of a tool instance within a scope.
#[derive(Debug, Clone)]
pub struct Materialization {
    /// Unique id for this materialization.
    pub id: ToolId,
    /// The provider that registered this tool.
    pub provider_id: ProviderId,
    /// Visibility mode (always vs. dynamic).
    pub visibility: ToolVisibility,
    /// Whether the entire materialization is disabled.
    pub wholly_disabled: bool,
    /// Where the tool came from (builtin / plugin / ephemeral).
    pub provenance: ToolProvenance,
    /// Optional forked child scope.
    pub scope_fork: Option<Arc<ScopeRef>>,
}

impl Materialization {
    /// Create a new materialization with the given identity fields.
    #[must_use]
    pub fn new(
        id: ToolId,
        provider_id: ProviderId,
        visibility: ToolVisibility,
        provenance: ToolProvenance,
    ) -> Self {
        Self {
            id,
            provider_id,
            visibility,
            wholly_disabled: false,
            provenance,
            scope_fork: None,
        }
    }

    /// Set the `wholly_disabled` flag.
    #[must_use]
    pub fn with_wholly_disabled(mut self, disabled: bool) -> Self {
        self.wholly_disabled = disabled;
        self
    }

    /// Set the scope fork.
    #[must_use]
    pub fn with_scope_fork(mut self, fork: Arc<ScopeRef>) -> Self {
        self.scope_fork = Some(fork);
        self
    }
}

/// A lightweight reference to a scope fork.
///
/// Uses `Weak<ScopeRef>` for the parent reference so that dropping
/// the parent scope doesn't prevent cleanup.
#[derive(Debug)]
pub struct ScopeRef {
    /// Name of this scope fork.
    pub name: String,
    /// Weak reference to the parent scope.
    pub parent: Option<std::sync::Weak<ScopeRef>>,
}

impl ScopeRef {
    /// Create a root scope (no parent).
    #[must_use]
    pub fn root(name: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            name: name.into(),
            parent: None,
        })
    }

    /// Fork a child scope from this parent.
    #[must_use]
    pub fn fork(self: &Arc<Self>, name: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            name: name.into(),
            parent: Some(Arc::downgrade(self)),
        })
    }

    /// Check whether the parent scope is still alive.
    pub fn parent_alive(&self) -> bool {
        self.parent.as_ref().is_none_or(|w| w.upgrade().is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialization_new() {
        let id = ToolId::new();
        let pid = ProviderId::new("test");
        let mat = Materialization::new(
            id,
            pid,
            ToolVisibility::Always,
            ToolProvenance::Builtin,
        );
        assert_eq!(mat.id, id);
        assert_eq!(mat.provider_id, pid);
        assert!(!mat.wholly_disabled);
        assert!(mat.scope_fork.is_none());
    }

    #[test]
    fn materialization_builder_pattern() {
        let mat = Materialization::new(
            ToolId::new(),
            ProviderId::new("bash"),
            ToolVisibility::Always,
            ToolProvenance::Builtin,
        )
        .with_wholly_disabled(true);
        assert!(mat.wholly_disabled);
    }

    #[test]
    fn scope_fork_parent_alive() {
        let root = ScopeRef::root("root");
        let child = root.fork("child");
        assert!(child.parent_alive());
        assert_eq!(child.name, "child");
    }

    #[test]
    fn scope_fork_parent_dropped() {
        let child = {
            let root = ScopeRef::root("root");
            root.fork("child")
        };
        // root is dropped; parent should be dead.
        assert!(!child.parent_alive());
    }

    #[test]
    fn scope_ref_root_has_no_parent() {
        let root = ScopeRef::root("root");
        assert!(root.parent.is_none());
        assert!(root.parent_alive());
    }
}
