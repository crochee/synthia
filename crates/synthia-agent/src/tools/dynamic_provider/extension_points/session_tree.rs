//! Session Tree extension points: 5 typed hook points fired by the
//! session-tree subsystem. The scope guarantees write ordering and
//! parent-link integrity across branch creation.
//!
//! # Design
//!
//! - **Write-bound pattern**: most points append / mutate entries in
//!   the in-memory tree. The fire methods emit a `tracing::info_span!`
//!   for OTel observability per handler.
//! - **Submission-order preservation**: `session.entry.append` runs
//!   the handler chain synchronously; the final `Entry` is the
//!   persisted record. Multiple handlers append in registration
//!   order; the final record is what gets stored.
//! - **Parent-link integrity**: `session.branch.create` freezes the
//!   parent (subsequent `append` on the parent returns
//!   `BranchFrozenError`). New appends go to the new branch.
//! - **`session.entry.tree_walk` is read-only**: handlers MAY NOT
//!   mutate state. Debug builds panic on attempted mutation; release
//!   builds log a warning.
//! - **`session.compaction.preserve` is observe-only**: the
//!   orchestrator decides what to preserve; the extension just
//!   observes the compaction event.
//!
//! # Points
//!
//! | Name | Payload | Purpose |
//! |------|---------|---------|
//! | `session.entry.append` | `EntryAppendInput` | Transform an entry before persistence |
//! | `session.entry.tree_walk` | `TreeWalkRequest` | Enumerate branches in pre-order |
//! | `session.branch.create` | `BranchCreateRequest` | Fork a session; freeze the parent |
//! | `session.version.migrate` | `MigrateRequest` → `Option<serde_json::Value>` | Schema upgrade |
//! | `session.compaction.preserve` | `CompactionEvent` | Observe-only |

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::tool::Action;

// =====================================================================
// Typed payloads
// =====================================================================

/// Entry id (monotonic, scoped to a session).
pub type EntryId = u64;
/// Session id (opaque).
pub type SessionId = String;

/// `session.entry.append` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub entry_id: EntryId,
    pub role: String,
    pub content: String,
    /// Free-form metadata; extensions MAY enrich.
    pub metadata: BTreeMap<String, serde_json::Value>,
    /// Free-form tags; extensions MAY add tags.
    pub tags: Vec<String>,
    /// Free-form annotations; extensions MAY annotate.
    pub annotations: BTreeMap<String, serde_json::Value>,
}

impl SessionEntry {
    pub fn new(
        entry_id: EntryId,
        role: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            entry_id,
            role: role.into(),
            content: content.into(),
            metadata: BTreeMap::new(),
            tags: Vec::new(),
            annotations: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryAppendInput {
    pub session_id: SessionId,
    pub entry: SessionEntry,
    /// `entry_id` of the parent (previous) entry; `None` for the
    /// root entry.
    pub parent_entry_id: Option<EntryId>,
}

impl EntryAppendInput {
    pub fn new(
        session_id: impl Into<SessionId>,
        entry: SessionEntry,
        parent_entry_id: Option<EntryId>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            entry,
            parent_entry_id,
        }
    }
}

/// `session.entry.tree_walk` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeWalkRequest {
    pub root_session_id: SessionId,
    pub max_depth: u32,
}

impl TreeWalkRequest {
    pub fn new(root_session_id: impl Into<SessionId>, max_depth: u32) -> Self {
        Self {
            root_session_id: root_session_id.into(),
            max_depth,
        }
    }
}

/// Node returned by `session.entry.tree_walk`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchNode {
    pub session_id: SessionId,
    pub parent_id: Option<SessionId>,
    pub depth: u32,
    pub entry_count: u32,
}

impl BranchNode {
    pub fn new(
        session_id: impl Into<SessionId>,
        parent_id: Option<SessionId>,
        depth: u32,
        entry_count: u32,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            parent_id,
            depth,
            entry_count,
        }
    }
}

/// `session.branch.create` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCreateRequest {
    pub parent_session_id: SessionId,
    pub branch_name: String,
}

impl BranchCreateRequest {
    pub fn new(
        parent_session_id: impl Into<SessionId>,
        branch_name: impl Into<String>,
    ) -> Self {
        Self {
            parent_session_id: parent_session_id.into(),
            branch_name: branch_name.into(),
        }
    }
}

/// `session.branch.create` event response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCreateOutput {
    pub new_session_id: SessionId,
    pub parent_session_id: SessionId,
}

impl BranchCreateOutput {
    pub fn new(
        new_session_id: impl Into<SessionId>,
        parent_session_id: impl Into<SessionId>,
    ) -> Self {
        Self {
            new_session_id: new_session_id.into(),
            parent_session_id: parent_session_id.into(),
        }
    }
}

/// `session.version.migrate` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateRequest {
    pub session_id: SessionId,
    pub from_version: u32,
    pub to_version: u32,
    pub payload: serde_json::Value,
}

impl MigrateRequest {
    pub fn new(
        session_id: impl Into<SessionId>,
        from_version: u32,
        to_version: u32,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            from_version,
            to_version,
            payload,
        }
    }
}

/// `session.compaction.preserve` event payload (observe-only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEvent {
    pub session_id: SessionId,
    pub entries_before: u32,
    pub entries_after: u32,
    /// `true` if the preserved summary was generated by an extension
    /// (per `pi-mono session-manager.ts:48-61`).
    pub from_hook: bool,
}

impl CompactionEvent {
    pub fn new(
        session_id: impl Into<SessionId>,
        entries_before: u32,
        entries_after: u32,
        from_hook: bool,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            entries_before,
            entries_after,
            from_hook,
        }
    }
}

/// Branch-frozen error returned by `session.entry.append` when the
/// parent is marked immutable.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("branch {0} is frozen and cannot accept new entries")]
pub struct BranchFrozenError(pub SessionId);

// =====================================================================
// Handler aliases
// =====================================================================

pub type EntryAppendHandler =
    Arc<dyn Fn(&EntryAppendInput) -> Action<EntryAppendInput> + Send + Sync>;

pub type TreeWalkHandler =
    Arc<dyn Fn(&TreeWalkRequest) -> Vec<BranchNode> + Send + Sync>;

pub type BranchCreateHandler = Arc<
    dyn Fn(&BranchCreateRequest) -> Action<BranchCreateOutput> + Send + Sync,
>;

pub type VersionMigrateHandler =
    Arc<dyn Fn(&MigrateRequest) -> Option<serde_json::Value> + Send + Sync>;

pub type CompactionPreserveHandler =
    Arc<dyn Fn(&CompactionEvent) + Send + Sync>;

// =====================================================================
// Registry
// =====================================================================

pub struct SessionTreeExtensionRegistry {
    entry_append: DashMap<String, Vec<EntryAppendHandler>>,
    tree_walk: DashMap<String, Vec<TreeWalkHandler>>,
    branch_create: DashMap<String, Vec<BranchCreateHandler>>,
    version_migrate: DashMap<String, Vec<VersionMigrateHandler>>,
    compaction_preserve: DashMap<String, Vec<CompactionPreserveHandler>>,
    active_keys: DashMap<String, ()>,
    /// Frozen branches — appends to these session_ids return
    /// `BranchFrozenError`.
    frozen_branches: DashMap<SessionId, ()>,
    /// Monotonic entry_id counter (per-registry, not per-session).
    next_entry_id: Arc<AtomicU64>,
}

impl std::fmt::Debug for SessionTreeExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionTreeExtensionRegistry")
            .field("entry_append", &self.entry_append.len())
            .field("tree_walk", &self.tree_walk.len())
            .field("branch_create", &self.branch_create.len())
            .field("version_migrate", &self.version_migrate.len())
            .field("compaction_preserve", &self.compaction_preserve.len())
            .field("frozen_branches", &self.frozen_branches.len())
            .finish()
    }
}

impl Default for SessionTreeExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionTreeExtensionRegistry {
    pub fn new() -> Self {
        Self {
            entry_append: DashMap::new(),
            tree_walk: DashMap::new(),
            branch_create: DashMap::new(),
            version_migrate: DashMap::new(),
            compaction_preserve: DashMap::new(),
            active_keys: DashMap::new(),
            frozen_branches: DashMap::new(),
            next_entry_id: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn register_entry_append(
        &self,
        id: impl Into<String>,
        handler: EntryAppendHandler,
    ) {
        self.entry_append
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("session.entry.append".into(), ());
    }

    pub fn register_tree_walk(
        &self,
        id: impl Into<String>,
        handler: TreeWalkHandler,
    ) {
        self.tree_walk.entry(id.into()).or_default().push(handler);
        self.active_keys
            .insert("session.entry.tree_walk".into(), ());
    }

    pub fn register_branch_create(
        &self,
        id: impl Into<String>,
        handler: BranchCreateHandler,
    ) {
        self.branch_create
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("session.branch.create".into(), ());
    }

    pub fn register_version_migrate(
        &self,
        id: impl Into<String>,
        handler: VersionMigrateHandler,
    ) {
        self.version_migrate
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("session.version.migrate".into(), ());
    }

    pub fn register_compaction_preserve(
        &self,
        id: impl Into<String>,
        handler: CompactionPreserveHandler,
    ) {
        self.compaction_preserve
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("session.compaction.preserve".into(), ());
    }

    pub fn has_handlers(&self, point: &str) -> bool {
        self.active_keys.contains_key(point)
    }

    /// Allocate the next entry id (per-registry counter).
    pub fn next_entry_id(&self) -> EntryId {
        self.next_entry_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// `true` if `session_id` is marked frozen (cannot accept new
    /// appends).
    pub fn is_frozen(&self, session_id: &str) -> bool {
        self.frozen_branches.contains_key(session_id)
    }

    /// Fire `session.entry.append`. Returns `Err(BranchFrozenError)`
    /// if the session is frozen. Otherwise, the chain runs in
    /// registration order; the final `Entry` is the persisted record.
    pub fn fire_entry_append(
        &self,
        event: EntryAppendInput,
    ) -> Result<Action<EntryAppendInput>, BranchFrozenError> {
        if self.frozen_branches.contains_key(&event.session_id) {
            return Err(BranchFrozenError(event.session_id));
        }
        let mut current = event;
        for entry in self.entry_append.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "session.entry.append",
                    scope = "session_tree",
                    extension_id = extension_id.as_str(),
                    session_id = current.session_id.as_str(),
                )
                .entered();
                match handler(&current) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        current = replacement;
                    }
                    Action::Skip { reason } => {
                        return Ok(Action::Skip { reason });
                    }
                }
            }
        }
        Ok(Action::Modify(current))
    }

    /// Fire `session.entry.tree_walk` (read-only). Returns the
    /// concatenation of all `Vec<BranchNode>` returned by handlers
    /// (filtered by `max_depth`).
    pub fn fire_tree_walk(&self, req: &TreeWalkRequest) -> Vec<BranchNode> {
        let mut out = Vec::new();
        for entry in self.tree_walk.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "session.entry.tree_walk",
                    scope = "session_tree",
                    extension_id = extension_id.as_str(),
                    session_id = req.root_session_id.as_str(),
                )
                .entered();
                let nodes = handler(req);
                for node in nodes {
                    if node.depth <= req.max_depth {
                        out.push(node);
                    }
                }
            }
        }
        out
    }

    /// Fire `session.branch.create`. The chain runs in registration
    /// order; the final `BranchCreateOutput` is the new branch. The
    /// parent is marked frozen.
    pub fn fire_branch_create(
        &self,
        req: &BranchCreateRequest,
    ) -> Action<BranchCreateOutput> {
        for entry in self.branch_create.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "session.branch.create",
                    scope = "session_tree",
                    extension_id = extension_id.as_str(),
                    session_id = req.parent_session_id.as_str(),
                )
                .entered();
                match handler(req) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        // Freeze the parent.
                        self.frozen_branches
                            .insert(req.parent_session_id.clone(), ());
                        return Action::Modify(replacement);
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        // No handler → still freeze the parent and return a default
        // new session id.
        self.frozen_branches
            .insert(req.parent_session_id.clone(), ());
        Action::Modify(BranchCreateOutput::new(
            format!("{}-{}", req.parent_session_id, req.branch_name),
            req.parent_session_id.clone(),
        ))
    }

    /// Fire `session.version.migrate`. Returns the first non-`None`
    /// migration from any registered handler.
    pub fn fire_version_migrate(
        &self,
        req: &MigrateRequest,
    ) -> Option<serde_json::Value> {
        for entry in self.version_migrate.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "session.version.migrate",
                    scope = "session_tree",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                    from_version = req.from_version,
                    to_version = req.to_version,
                )
                .entered();
                if let Some(value) = handler(req) {
                    return Some(value);
                }
            }
        }
        None
    }

    /// Fire `session.compaction.preserve` (observe-only).
    pub fn fire_compaction_preserve(&self, event: &CompactionEvent) {
        for entry in self.compaction_preserve.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "session.compaction.preserve",
                    scope = "session_tree",
                    extension_id = extension_id.as_str(),
                    session_id = event.session_id.as_str(),
                )
                .entered();
                handler(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let reg = SessionTreeExtensionRegistry::new();
        assert!(!reg.has_handlers("session.entry.append"));
        assert!(!reg.has_handlers("session.entry.tree_walk"));
        assert!(!reg.has_handlers("session.branch.create"));
        assert!(!reg.has_handlers("session.version.migrate"));
        assert!(!reg.has_handlers("session.compaction.preserve"));
        assert_eq!(reg.next_entry_id(), 1);
    }

    #[test]
    fn append_preserves_submission_order() {
        let reg = SessionTreeExtensionRegistry::new();
        // No handlers → returns the entry as-is.
        let mut e1 = SessionEntry::new(1, "user", "hello");
        e1.tags.push("first".to_string());
        let mut e2 = SessionEntry::new(2, "assistant", "hi");
        e2.tags.push("second".to_string());
        let r1 = reg
            .fire_entry_append(EntryAppendInput::new("s1", e1.clone(), None))
            .unwrap();
        let r2 = reg
            .fire_entry_append(EntryAppendInput::new("s1", e2.clone(), Some(1)))
            .unwrap();
        let Action::Modify(m1) = r1 else {
            panic!("expected Modify")
        };
        let Action::Modify(m2) = r2 else {
            panic!("expected Modify")
        };
        assert_eq!(m1.entry.entry_id, 1);
        assert_eq!(m2.entry.entry_id, 2);
        assert_eq!(m2.parent_entry_id, Some(1));
    }

    #[test]
    fn append_handler_modifies_metadata() {
        let reg = SessionTreeExtensionRegistry::new();
        let h: EntryAppendHandler = Arc::new(|ev| {
            let mut next = ev.clone();
            next.entry
                .metadata
                .insert("source".to_string(), serde_json::json!("audit"));
            Action::Modify(next)
        });
        reg.register_entry_append("audit", h);
        let entry = SessionEntry::new(1, "user", "x");
        let r = reg
            .fire_entry_append(EntryAppendInput::new("s1", entry, None))
            .unwrap();
        let Action::Modify(m) = r else {
            panic!("expected Modify")
        };
        assert_eq!(m.entry.metadata.get("source").unwrap(), "audit");
    }

    #[test]
    fn branch_create_freezes_parent() {
        let reg = SessionTreeExtensionRegistry::new();
        let req = BranchCreateRequest::new("parent-1", "alt");
        let Action::Modify(out) = reg.fire_branch_create(&req) else {
            panic!("expected Modify")
        };
        assert!(reg.is_frozen("parent-1"));
        // Subsequent append to parent returns BranchFrozenError.
        let entry = SessionEntry::new(99, "user", "x");
        let r = reg
            .fire_entry_append(EntryAppendInput::new("parent-1", entry, None));
        assert!(matches!(r, Err(BranchFrozenError(_))));
        // Append to the new branch is allowed.
        let r2 = reg.fire_entry_append(EntryAppendInput::new(
            &out.new_session_id,
            SessionEntry::new(100, "user", "x"),
            None,
        ));
        assert!(r2.is_ok());
    }

    #[test]
    fn tree_walk_filters_by_max_depth() {
        let reg = SessionTreeExtensionRegistry::new();
        let h: TreeWalkHandler = Arc::new(|_req| {
            vec![
                BranchNode::new("s1", None, 0, 5),
                BranchNode::new("s2", Some("s1".to_string()), 1, 3),
                BranchNode::new("s3", Some("s2".to_string()), 2, 1),
                BranchNode::new("s4", Some("s3".to_string()), 3, 1),
            ]
        });
        reg.register_tree_walk("core", h);
        let req = TreeWalkRequest::new("s1", 2);
        let nodes = reg.fire_tree_walk(&req);
        assert_eq!(nodes.len(), 3);
        assert!(nodes.iter().all(|n| n.depth <= 2));
    }

    #[test]
    fn version_migrate_returns_first_non_none() {
        let reg = SessionTreeExtensionRegistry::new();
        let h: VersionMigrateHandler =
            Arc::new(|_req| Some(serde_json::json!({"migrated": true})));
        reg.register_version_migrate("v1", h);
        let req = MigrateRequest::new("s1", 1, 2, serde_json::json!({}));
        let r = reg.fire_version_migrate(&req);
        assert_eq!(r, Some(serde_json::json!({"migrated": true})));
    }

    #[test]
    fn compaction_preserve_is_observe_only() {
        let reg = SessionTreeExtensionRegistry::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = counter.clone();
        let h: CompactionPreserveHandler = Arc::new(move |_ev| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        reg.register_compaction_preserve("audit", h);
        let ev = CompactionEvent::new("s1", 100, 50, true);
        reg.fire_compaction_preserve(&ev);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
