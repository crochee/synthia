//! `SessionTree` — in-memory representation of append-only JSONL.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use synthia_protocol::{MessageId, SessionId};

use crate::entry::SessionEntry;

/// `Ord`-wrapper around `MessageId` for use as a `BTreeMap` key.
///
/// `synthia_protocol::MessageId` (a UUID newtype) does not derive `Ord`,
/// but `BTreeMap` requires `Ord`. We wrap it locally and implement ordering
/// via the inner UUID to preserve the spec-mandated deterministic ordering
/// without modifying the protocol crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageKey(pub MessageId);

impl MessageKey {
    #[inline]
    pub fn new() -> Self {
        Self(MessageId::new())
    }
}

impl Default for MessageKey {
    fn default() -> Self {
        Self::new()
    }
}

impl From<MessageId> for MessageKey {
    fn from(id: MessageId) -> Self {
        Self(id)
    }
}

impl Ord for MessageKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.0.cmp(&other.0.0)
    }
}

impl PartialOrd for MessageKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTree {
    /// All entries keyed by MessageId (BTreeMap for deterministic ordering).
    pub entries: BTreeMap<MessageKey, SessionEntry>,
    /// Children index: parent_id → child_ids.
    pub children: HashMap<MessageKey, Vec<MessageKey>>,
    pub root: SessionId,
    pub leaf: MessageId,
    /// Cached path from root to leaf (rebuilt on leaf change).
    pub paths_from_root: Vec<MessageId>,
}

impl SessionTree {
    pub fn new(root: SessionId, leaf: MessageId) -> Self {
        Self {
            entries: BTreeMap::new(),
            children: HashMap::new(),
            root,
            leaf,
            paths_from_root: vec![leaf],
        }
    }

    pub fn append(&mut self, entry: SessionEntry) {
        let id = MessageKey(entry.id_unwrap());
        let parent_id = entry.parent_id();
        self.entries.insert(id, entry);
        if let Some(p) = parent_id {
            self.children.entry(MessageKey(p)).or_default().push(id);
        }
        self.leaf = id.0;
        self.rebuild_paths();
    }

    pub fn set_leaf(&mut self, new_leaf: MessageId) {
        self.leaf = new_leaf;
        self.rebuild_paths();
    }

    fn rebuild_paths(&mut self) {
        let mut path = Vec::new();
        let mut current = Some(MessageKey(self.leaf));
        while let Some(id) = current {
            path.push(id.0);
            current = self
                .entries
                .get(&id)
                .and_then(|e| e.parent_id())
                .map(MessageKey);
        }
        path.reverse();
        self.paths_from_root = path;
    }

    pub fn depth(&self) -> usize {
        self.paths_from_root.len()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn empty_tree_has_leaf_path() {
        let root = SessionId::new();
        let leaf = MessageId::new();
        let tree = SessionTree::new(root, leaf);
        assert_eq!(tree.depth(), 1);
    }

    #[test]
    fn append_extends_path() {
        let root = SessionId::new();
        let mut tree = SessionTree::new(root, MessageId::new());

        let m1 = MessageId::new();
        tree.append(SessionEntry::Message {
            id: m1,
            parent_message_id: Some(tree.leaf),
            role: "user".to_string(),
            parts: vec![],
            time: Utc::now(),
            agent_name: None,
            model_id: None,
        });
        assert_eq!(tree.depth(), 2);
        assert_eq!(tree.leaf, m1);
    }
}
