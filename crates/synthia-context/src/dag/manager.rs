//! The two-phase (leaf + condensed) hierarchical DAG context
//! manager — the compaction engine itself.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use futures::future::join_all;
use parking_lot::Mutex;
use synthia_provider::{Content, Message, Role};
use ulid::Ulid;

use super::{
    COMPACTION_TAG,
    SummaryId,
    config::DagConfig,
    prompts::{
        build_condensed_d1_prompt,
        build_condensed_d2_plus_prompt,
        build_leaf_prompt,
    },
    serialise::{
        chunk_by_tokens,
        estimate_tokens_of,
        message_text,
        serialise_message_for_prompt,
    },
    store::{DagNode, DagStore},
};
use crate::{
    context_manager::{AgentState, ContextManager},
    summarizing::SummariseFn,
};

/// Two-phase (leaf + condensed) hierarchical DAG context manager.
pub struct DagContextManager {
    config: DagConfig,
    summarise: SummariseFn,
    store: Arc<DagStore>,
    /// How many leading raw messages have already been folded
    /// into D0 leaves across every `prepare` call. Messages are
    /// only ever appended between calls, so a plain counter is
    /// sufficient (no content hashing needed).
    compacted_count: Mutex<usize>,
}

impl DagContextManager {
    pub fn new(summarise: SummariseFn) -> Self {
        Self::with_config(DagConfig::default(), summarise)
    }

    pub fn with_config(config: DagConfig, summarise: SummariseFn) -> Self {
        Self {
            config: config.validated(),
            summarise,
            store: Arc::new(DagStore::new()),
            compacted_count: Mutex::new(0),
        }
    }

    /// Borrow the underlying DAG. Callers wire this into
    /// `lcm_describe` / `lcm_expand` tools so the model can
    /// query summary contents by id.
    pub fn store(&self) -> &Arc<DagStore> {
        &self.store
    }

    /// `pi-lcm`-style describe: latest node per depth + stats.
    pub fn describe(&self) -> String {
        let latest = self.store.latest_per_depth();
        let total = self.store.nodes().len();
        if latest.is_empty() {
            return format!(
                "[lcm_describe] no summaries yet ({total} nodes, {} raw messages processed)",
                self.compacted_count.lock()
            );
        }
        let mut out =
            format!("[lcm_describe] {total} DAG nodes; latest per depth:\n");
        for n in &latest {
            let preview: String = n.text.chars().take(80).collect();
            out.push_str(&format!(
                "  D{} {} ({} children): {preview}\n",
                n.depth,
                &n.id[..8.min(n.id.len())],
                n.children.len()
            ));
        }
        out
    }

    /// `pi-lcm`-style expand: recursively concatenate a node's
    /// subtree (summaries + raw originals) up to an ~8K-token
    /// cap. Cycle-safe.
    pub fn expand(&self, id: &str) -> Option<String> {
        let mut visited: std::collections::HashSet<SummaryId> =
            std::collections::HashSet::new();
        let mut out = String::new();
        self.expand_inner(id, &mut visited, &mut out);
        if out.is_empty() { None } else { Some(out) }
    }

    fn expand_inner(
        &self,
        id: &str,
        visited: &mut std::collections::HashSet<SummaryId>,
        out: &mut String,
    ) {
        if !visited.insert(id.to_string()) {
            return;
        }
        let cap_chars = 8_000 * 4;
        if let Some(node) = self.store.node(id) {
            if out.len() < cap_chars {
                out.push_str(&format!(
                    "\n--- D{} {} ---\n{}\n",
                    node.depth, node.id, node.text
                ));
            }
            for child in &node.children {
                self.expand_inner(child, visited, out);
            }
            return;
        }
        // Not a node → minted message id: recover the raw original.
        if out.len() < cap_chars
            && let Some(text) = self.store.raw_text(id)
        {
            out.push_str(&format!(
                "\n--- raw {} ---\n{text}\n",
                &id[..8.min(id.len())]
            ));
        }
    }

    async fn leaf_pass(
        &self,
        chunks: Vec<Vec<SummaryId>>,
        serialised: &HashMap<SummaryId, String>,
    ) {
        let concurrency = self.config.leaf_pass_concurrency;
        for wave in chunks.chunks(concurrency) {
            let futures: Vec<_> = wave
                .iter()
                .map(|chunk| {
                    let summarise = Arc::clone(&self.summarise);
                    let combined = chunk
                        .iter()
                        .filter_map(|id| serialised.get(id).cloned())
                        .collect::<Vec<_>>()
                        .join("\n\n---\n\n");
                    let prompt = build_leaf_prompt(&combined);
                    async move { (chunk.clone(), summarise(&prompt).await) }
                })
                .collect();
            for (chunk_ids, summary) in join_all(futures).await {
                // pi-lcm Fix 11: on summariser failure do NOT
                // compact — leave the range for the next cycle.
                let Some(summary_text) = summary else {
                    continue;
                };
                self.store.insert(DagNode {
                    id: Ulid::generate().to_string(),
                    depth: 0,
                    text: summary_text,
                    children: chunk_ids,
                    consumed: false,
                });
            }
        }
    }

    async fn condensed_pass(&self) {
        let mut did_condense = true;
        let mut passes: u32 = 0;

        while did_condense && passes < self.config.max_passes {
            did_condense = false;
            passes += 1;

            for depth in 0..self.config.max_depth {
                let unconsumed = self.store.unconsumed_at_depth(depth);
                if unconsumed.len() < self.config.condensation_threshold {
                    continue;
                }

                let to_condense: Vec<DagNode> = unconsumed
                    .into_iter()
                    .take(self.config.condensation_threshold)
                    .collect();

                let combined = to_condense
                    .iter()
                    .map(|n| n.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");

                let prompt = if depth + 1 == 1 {
                    build_condensed_d1_prompt(&combined)
                } else {
                    build_condensed_d2_plus_prompt(depth + 1, &combined)
                };

                // pi-lcm: abort the cascade on failure so partial
                // nodes never point at unconsumed children.
                let Some(summary_text) = (self.summarise)(&prompt).await else {
                    return;
                };

                let child_ids: Vec<SummaryId> =
                    to_condense.iter().map(|n| n.id.clone()).collect();
                self.store.insert(DagNode {
                    id: Ulid::generate().to_string(),
                    depth: depth + 1,
                    text: summary_text,
                    children: child_ids.clone(),
                    consumed: false,
                });
                self.store.mark_consumed(&child_ids);
                did_condense = true;
            }
        }
    }

    /// Render the assembled conversation-history view the way
    /// `pi-lcm`'s `assembleSummary` does: deepest summaries
    /// first, then the most recent D0, then a drill-down id list.
    fn assemble_view(&self) -> String {
        let stats = format!(
            "{} DAG nodes; {} raw messages processed",
            self.store.nodes().len(),
            self.compacted_count.lock()
        );
        let latest = self.store.latest_per_depth();
        if latest.is_empty() {
            return format!(
                "## Conversation History (Lossless DAG)\n{stats}\n\nNo summaries yet."
            );
        }

        let mut parts: Vec<String> = vec![
            "## Conversation History (Lossless DAG)".to_string(),
            stats,
            String::new(),
        ];

        let token_cap = self.config.max_summary_tokens;
        let mut tokens_used = estimate_tokens_of(&parts.join("\n"));

        let deep: Vec<&DagNode> =
            latest.iter().filter(|n| n.depth >= 1).collect();
        if !deep.is_empty() {
            parts.push("### High-Level Summary".to_string());
            for n in &deep {
                let cost = estimate_tokens_of(&n.text) + 20;
                if tokens_used + cost > token_cap {
                    break;
                }
                parts.push(n.text.clone());
                parts.push(String::new());
                tokens_used += cost;
            }
        }

        if let Some(n) = latest.iter().find(|n| n.depth == 0) {
            let cost = estimate_tokens_of(&n.text) + 30;
            if tokens_used + cost <= token_cap {
                parts.push("### Recent Activity".to_string());
                parts.push(n.text.clone());
                parts.push(String::new());
            }
        }

        let all = self.store.nodes();
        if !all.is_empty() {
            parts.push("### Summary IDs for Drill-Down".to_string());
            for n in &all {
                let preview: String = n.text.chars().take(60).collect();
                parts.push(format!(
                    "- {} (D{}): \"{preview}…\"",
                    &n.id[..8.min(n.id.len())],
                    n.depth
                ));
            }
        }

        parts.join("\n")
    }

    /// Strip the previously injected marker message, if any.
    fn strip_marker(messages: &mut Vec<Message>) {
        messages.retain(|m| {
            !(m.role == Role::Assistant
                && message_text(m).contains(&format!("<{COMPACTION_TAG}")))
        });
    }
}

#[async_trait]
impl ContextManager for DagContextManager {
    async fn prepare(
        &self,
        messages: &mut Vec<Message>,
        state: &mut AgentState,
    ) {
        state.last_truncated = false;
        state.estimated_tokens = self.estimate_tokens(messages);

        if messages.len() < self.config.min_messages_for_compaction {
            return;
        }

        // Remove our own previous summary so the fold always
        // operates on raw turns only.
        Self::strip_marker(messages);
        state.estimated_tokens = self.estimate_tokens(messages);

        let compacted = *self.compacted_count.lock();
        let new_len = messages.len().saturating_sub(compacted);
        if new_len == 0 && state.estimated_tokens <= state.context_window {
            // Nothing new since the last cycle and the window
            // already fits — no work to do.
            return;
        }

        // Mint ids for the new messages and remember their
        // serialised originals (lossless recovery path).
        let new_ids = self.store.mint_message_ids(new_len);
        let raw_pairs: Vec<(SummaryId, String)> = messages[compacted..]
            .iter()
            .zip(new_ids.iter())
            .map(|(msg, id)| (id.clone(), serialise_message_for_prompt(msg)))
            .collect();
        let serialised: HashMap<SummaryId, String> =
            raw_pairs.clone().into_iter().collect();
        self.store.store_raw_texts(raw_pairs);
        *self.compacted_count.lock() = messages.len();

        // Chunk by token budget and run the two passes — only
        // when there is fresh material to fold.
        if !new_ids.is_empty() {
            let chunks = chunk_by_tokens(
                &new_ids,
                &serialised,
                self.config.leaf_chunk_tokens,
            );
            let before_nodes = self.store.nodes().len();
            self.leaf_pass(chunks, &serialised).await;
            self.condensed_pass().await;

            // If every summariser call failed, no new nodes
            // exist: keep the raw history visible (pi-lcm Fix
            // 11). With no new ids we never attempted calls, so
            // the existing DAG still backs a re-emit.
            if self.store.nodes().len() == before_nodes {
                return;
            }
        }

        // No summaries at all (first-cycle total failure) — raw
        // history stays visible.
        if self.store.nodes().is_empty() {
            return;
        }

        let view = self.assemble_view();
        let body = format!("<{COMPACTION_TAG}>\n{view}\n</{COMPACTION_TAG}>");
        messages.clear();
        messages.push(Message::new(Role::Assistant, Content::text(body)));
        state.last_truncated = true;
        state.estimated_tokens = self.estimate_tokens(messages);
    }
}
