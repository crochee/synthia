//! Hierarchical-DAG context-window manager — Rust port of
//! [`pi-lcm`](https://github.com/codexstar69/pi-lcm)'s
//! `CompactionEngine` + `assembleSummary`.
//!
//! ## Behaviour
//!
//! `prepare` performs two passes when the conversation has grown
//! past `state.context_window`:
//!
//! 1. **Leaf pass** — chunk the not-yet-compacted messages into
//!    `leaf_chunk_tokens`-sized groups, summarise each chunk
//!    (parallel, `leaf_pass_concurrency` waves), and persist the
//!    summaries as D0 nodes whose children reference the raw
//!    message ids.
//! 2. **Condensed pass** — while any depth holds
//!    `>= condensation_threshold` unconsumed summaries, promote
//!    that many of them into one D-(n+1) summary. The cascade is
//!    bounded by `max_passes`.
//!
//! The visible history is then replaced by a single assistant
//! summary message tagged with `<lcm-compaction>`; the next
//! `prepare` strips that marker message before folding in the
//! newly accumulated turns.
//!
//! ## Losslessness
//!
//! `pi-lcm` keeps raw messages in SQLite. This port keeps every
//! serialised original in [`DagStore`]'s raw-text map, keyed by
//! the message id minted at compaction time, so [`expand`
//! `(DagContextManager::expand)`] can recover any original
//! content without a database. Durable persistence stays with
//! `synthia-session` (the JSONL sink already stores every
//! event); this DAG is the in-process recovery index.
//!
//! ## Mapping back to `pi-lcm`
//!
//! | `pi-lcm` | This crate |
//! |---|---|
//! | `LcmStore` (SQLite + FTS5) | `DagStore` in-memory maps |
//! | `summarize` (cheap model) | `SummariseFn` callback |
//! | `leafPass` (4-worker pool) | `leaf_pass` (join_all waves) |
//! | `condensedPass` (bounded cascade) | `condensed_pass` |
//! | `assembleSummary` | `assemble_view` |
//! | `lcm_describe` / `lcm_expand` | `describe` / `expand` |
//! | `markCompacted` failure-retry (Fix 11) | failed chunks skipped, retried next cycle |

//!
//! ## Module layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `config` | [`DagConfig`] — the knobs and their `pi-lcm` defaults. |
//! | `store` | [`DagNode`] + [`DagStore`] — the in-memory DAG and the lossless raw-text map. |
//! | `manager` | [`DagContextManager`] — the two passes and the `ContextManager` impl. |
//! | `prompts` | The three summariser prompts (mirror `pi-lcm/src/compaction/prompts.ts`). |
//! | `serialise` | Message serialisation, token estimation, and chunking. |

mod config;
mod manager;
mod prompts;
mod serialise;
mod store;

pub use config::DagConfig;
pub use manager::DagContextManager;
pub use store::{DagNode, DagStore};

/// XML marker that tags the assembled summary message injected
/// into the visible history. Matched on the next `prepare` to
/// strip the previous summary before folding new turns in.
pub const COMPACTION_TAG: &str = "lcm-compaction";

/// Synthetic id assigned to every DAG node and raw message.
pub type SummaryId = String;

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use synthia_provider::{Message, Role};

    use super::{serialise::message_text, *};
    use crate::{
        context_manager::{AgentState, ContextManager},
        summarizing::SummariseFn,
    };

    fn user_text(s: &str) -> Message {
        Message::user(s.to_string())
    }

    fn summarise_that_returns(s: String) -> SummariseFn {
        Arc::new(move |_input: &str| {
            let s = s.clone();
            Box::pin(async move { Some(s) })
        })
    }

    fn summarise_counting(counter: Arc<AtomicUsize>) -> SummariseFn {
        Arc::new(move |_| {
            let c = Arc::clone(&counter);
            Box::pin(async move {
                c.fetch_add(1, Ordering::SeqCst);
                Some("leaf".to_string())
            })
        })
    }

    #[test]
    fn config_default_matches_pi_lcm() {
        let cfg = DagConfig::default();
        assert_eq!(cfg.leaf_chunk_tokens, 4000);
        assert_eq!(cfg.condensation_threshold, 6);
        assert_eq!(cfg.max_depth, 5);
        assert_eq!(cfg.max_passes, 10);
        assert_eq!(cfg.leaf_pass_concurrency, 4);
        assert_eq!(cfg.min_messages_for_compaction, 10);
        assert_eq!(cfg.max_summary_tokens, 8000);
    }

    #[test]
    fn config_clamps_absurd_values() {
        let cfg = DagConfig {
            leaf_chunk_tokens: 1,
            condensation_threshold: 1,
            max_depth: 0,
            max_passes: 0,
            leaf_pass_concurrency: 0,
            min_messages_for_compaction: 0,
            max_summary_tokens: 0,
        }
        .validated();
        assert_eq!(cfg.leaf_chunk_tokens, 500);
        assert_eq!(cfg.condensation_threshold, 2);
        assert_eq!(cfg.max_depth, 1);
        assert_eq!(cfg.max_passes, 1);
        assert_eq!(cfg.leaf_pass_concurrency, 1);
        assert_eq!(cfg.min_messages_for_compaction, 2);
        assert_eq!(cfg.max_summary_tokens, 500);
    }

    #[tokio::test]
    async fn short_conversation_bypasses_dag() {
        let mgr =
            DagContextManager::new(summarise_that_returns("ignored".into()));
        let mut msgs: Vec<Message> =
            (0..5).map(|i| user_text(&format!("m{i}"))).collect();
        let before = msgs.clone();
        let mut state = AgentState::with_window(0);
        mgr.prepare(&mut msgs, &mut state).await;
        assert_eq!(msgs, before, "below min_messages_for_compaction → no-op");
        assert!(!state.last_truncated);
    }

    #[tokio::test]
    async fn long_conversation_collapses_to_marker_summary() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mgr = DagContextManager::with_config(
            DagConfig {
                leaf_chunk_tokens: 10,
                condensation_threshold: 3,
                min_messages_for_compaction: 5,
                ..DagConfig::default()
            },
            summarise_counting(Arc::clone(&counter)),
        );

        let mut msgs: Vec<Message> = (0..12)
            .map(|i| {
                user_text(&format!("{} ", "x".repeat(40) + &i.to_string()))
            })
            .collect();
        let mut state = AgentState::with_window(0);
        mgr.prepare(&mut msgs, &mut state).await;

        assert!(state.last_truncated);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::Assistant);
        let body = message_text(&msgs[0]);
        assert!(body.contains(&format!("<{COMPACTION_TAG}")));
        assert!(body.contains("Conversation History"));

        let nodes = mgr.store().nodes();
        assert!(nodes.iter().any(|n| n.depth == 0));
        assert!(counter.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn expand_recovers_raw_originals_losslessly() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mgr = DagContextManager::with_config(
            DagConfig {
                leaf_chunk_tokens: 10,
                min_messages_for_compaction: 4,
                ..DagConfig::default()
            },
            summarise_counting(counter),
        );
        let mut msgs: Vec<Message> = (0..6)
            .map(|i| user_text(&format!("unique-payload-{i}")))
            .collect();
        let mut state = AgentState::with_window(0);
        mgr.prepare(&mut msgs, &mut state).await;

        // Drill into any D0 node: its children are raw message
        // ids, so expansion must surface the original payloads.
        let d0 = mgr
            .store()
            .nodes()
            .into_iter()
            .find(|n| n.depth == 0)
            .expect("at least one D0 node");
        let expanded = mgr.expand(&d0.id).expect("expand produces text");
        assert!(
            expanded.contains("unique-payload-0"),
            "raw originals must be recoverable"
        );
    }

    #[tokio::test]
    async fn second_prepare_strips_previous_marker() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mgr = DagContextManager::with_config(
            DagConfig {
                leaf_chunk_tokens: 10,
                min_messages_for_compaction: 4,
                ..DagConfig::default()
            },
            summarise_counting(counter),
        );
        let mut msgs: Vec<Message> =
            (0..6).map(|i| user_text(&format!("turn-{i}"))).collect();
        let mut state = AgentState::with_window(0);
        mgr.prepare(&mut msgs, &mut state).await;
        assert_eq!(msgs.len(), 1);

        // New turns arrive after the summary message.
        for i in 0..6 {
            msgs.push(user_text(&format!("turn2-{i}")));
        }
        mgr.prepare(&mut msgs, &mut state).await;

        // Exactly one summary message, not a summary-of-summary
        // chain: the marker from cycle 1 was stripped before
        // folding.
        assert_eq!(msgs.len(), 1);
        let markers = message_text(&msgs[0])
            .matches(&format!("<{COMPACTION_TAG}"))
            .count();
        assert_eq!(markers, 1, "only the fresh marker may remain");
    }

    #[tokio::test]
    async fn summariser_total_failure_keeps_raw_history() {
        let fail: SummariseFn = Arc::new(|_| Box::pin(async { None }));
        let mgr = DagContextManager::with_config(
            DagConfig {
                leaf_chunk_tokens: 10,
                min_messages_for_compaction: 4,
                ..DagConfig::default()
            },
            fail,
        );
        let mut msgs: Vec<Message> =
            (0..6).map(|i| user_text(&format!("m{i}"))).collect();
        let before = msgs.clone();
        let mut state = AgentState::with_window(0);
        mgr.prepare(&mut msgs, &mut state).await;
        assert_eq!(msgs, before, "failed summariser must not destroy history");
        assert!(!state.last_truncated);
    }

    #[tokio::test]
    async fn expansion_resolves_manual_dag_chain() {
        let mgr = DagContextManager::new(summarise_that_returns("x".into()));
        mgr.store().insert(DagNode {
            id: "d0-a".into(),
            depth: 0,
            text: "raw A".into(),
            children: vec!["msg-a".into()],
            consumed: false,
        });
        mgr.store().insert(DagNode {
            id: "d0-b".into(),
            depth: 0,
            text: "raw B".into(),
            children: vec!["msg-b".into()],
            consumed: false,
        });
        mgr.store().insert(DagNode {
            id: "d1-a".into(),
            depth: 1,
            text: "combined".into(),
            children: vec!["d0-a".into(), "d0-b".into()],
            consumed: false,
        });

        let out = mgr.expand("d1-a").expect("expand should produce text");
        assert!(out.contains("combined"));
        assert!(out.contains("raw A"));
        assert!(out.contains("raw B"));
    }

    #[tokio::test]
    async fn expansion_is_cycle_safe() {
        let mgr = DagContextManager::new(summarise_that_returns("x".into()));
        mgr.store().insert(DagNode {
            id: "a".into(),
            depth: 0,
            text: "A".into(),
            children: vec!["b".into()],
            consumed: false,
        });
        mgr.store().insert(DagNode {
            id: "b".into(),
            depth: 0,
            text: "B".into(),
            children: vec!["a".into()],
            consumed: false,
        });
        let out = mgr.expand("a");
        assert!(out.is_some());
        assert!(out.unwrap().contains('A'));
    }
}
