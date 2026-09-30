//! The in-memory DAG: summary nodes, minted message ids, and
//! the serialised original text for every compacted message
//! (the lossless half of the port).

use std::collections::HashMap;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use super::SummaryId;

/// One node in the DAG. `depth` follows the `pi-lcm` convention:
///
/// - D0 = leaf summary (over a chunk of raw messages)
/// - D1+ = condensed summary (over a group of child summaries)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagNode {
    pub id: SummaryId,
    pub depth: u32,
    /// Plain-text summary produced by the LLM.
    pub text: String,
    /// ids of the children this node summarises — `DagNode` ids
    /// when `depth > 0`, raw message ids when `depth == 0`.
    pub children: Vec<SummaryId>,
    /// `true` once consumed by a parent at `depth + 1`. Mirrors
    /// `pi-lcm`'s `markCompacted` / unconsumed split.
    pub consumed: bool,
}

/// In-memory DAG backing store: summary nodes, minted message
/// ids, and the serialised original text for every compacted
/// message (the lossless half of the port).
#[derive(Default)]
pub struct DagStore {
    nodes: Mutex<HashMap<SummaryId, DagNode>>,
    message_ids: Mutex<Vec<SummaryId>>,
    raw_texts: Mutex<HashMap<SummaryId, String>>,
}

impl DagStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every summary node.
    pub fn nodes(&self) -> Vec<DagNode> {
        self.nodes.lock().values().cloned().collect()
    }

    /// Look up one summary node by id.
    pub fn node(&self, id: &str) -> Option<DagNode> {
        self.nodes.lock().get(id).cloned()
    }

    /// Latest (highest-id) node per depth, deepest first.
    pub fn latest_per_depth(&self) -> Vec<DagNode> {
        let nodes = self.nodes.lock();
        let mut by_depth: HashMap<u32, DagNode> = HashMap::new();
        for node in nodes.values() {
            match by_depth.get(&node.depth) {
                Some(entry) if entry.id >= node.id => {}
                _ => {
                    by_depth.insert(node.depth, node.clone());
                }
            }
        }
        drop(nodes);
        let mut out: Vec<DagNode> = by_depth.into_values().collect();
        out.sort_by_key(|n| std::cmp::Reverse(n.depth));
        out
    }

    /// Raw original text for a minted message id (lossless path).
    pub fn raw_text(&self, id: &str) -> Option<String> {
        self.raw_texts.lock().get(id).cloned()
    }

    pub(super) fn insert(&self, node: DagNode) {
        self.nodes.lock().insert(node.id.clone(), node);
    }

    pub(super) fn mark_consumed(&self, ids: &[SummaryId]) {
        let mut nodes = self.nodes.lock();
        for id in ids {
            if let Some(n) = nodes.get_mut(id) {
                n.consumed = true;
            }
        }
    }

    pub(super) fn unconsumed_at_depth(&self, depth: u32) -> Vec<DagNode> {
        self.nodes
            .lock()
            .values()
            .filter(|n| n.depth == depth && !n.consumed)
            .cloned()
            .collect()
    }

    pub(super) fn mint_message_ids(&self, count: usize) -> Vec<SummaryId> {
        let mut ids = self.message_ids.lock();
        let start = ids.len();
        for _ in 0..count {
            ids.push(Ulid::generate().to_string());
        }
        ids[start..].to_vec()
    }

    pub(super) fn store_raw_texts(&self, pairs: Vec<(SummaryId, String)>) {
        let mut raw = self.raw_texts.lock();
        for (id, text) in pairs {
            raw.insert(id, text);
        }
    }
}
