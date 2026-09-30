//! What the manager emits: one record per splice, and the
//! three-step lifecycle that makes a crash mid-compaction
//! detectable in the durable log.

use synthia_session::{CompactionOutcome, SurfaceToken};

/// Description of one compaction splice emitted by
/// [`super::SummarizingContextManager`].
#[derive(Debug, Clone)]
pub struct CompactionRecord {
    /// Inclusive start index in the pre-splice `messages` vector.
    pub start: usize,
    /// Exclusive end index in the pre-splice `messages` vector.
    pub end: usize,
    /// `start..end` indices that the splice replaced.
    pub source_indices: Vec<usize>,
    /// Durable identity of each replaced row, parallel to
    /// `source_indices`: the tool-call id of a tool-result message.
    ///
    /// The indices are positions in the live message vector, which a
    /// durable log cannot be trusted to reproduce (the loop prepends
    /// a system prompt that is never logged, and one assistant turn
    /// can span several log rows). A consumer that wants to persist
    /// the splice resolves these keys against the run's log instead;
    /// a `None` entry means the row has no durable identity and the
    /// splice must not be written with invented provenance.
    pub source_keys: Vec<Option<String>>,
    /// Summary text the splice substituted for the batch.
    pub summary_text: String,
}

/// One step of a compaction lifecycle (R29-Phase-J).
///
/// Fired to the callback installed by
/// [`super::SummarizingContextManager::with_compaction_lifecycle_emitter`].
/// The three steps exist so a durable log can pair them: a
/// `Start` with no matching `End` is the crash marker
/// (`synthia_session::orphaned_compactions` finds it), which is
/// strictly more informative than a lone "compaction happened"
/// record that cannot distinguish "crashed" from "succeeded".
///
/// The summary is a *separate* step from the end so a crash
/// between the LLM call and the splice leaves a `Start` +
/// `Summary` pair with no `End` — the model produced text but the
/// log never committed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionLifecycle {
    /// The manager is about to call the summariser.
    Start {
        /// Correlation id for this lifecycle.
        token: SurfaceToken,
    },
    /// The summariser returned usable text.
    Summary {
        /// Correlation id.
        token: SurfaceToken,
        /// The summary text.
        summary: String,
        /// Pre-splice `messages` positions the summary shadows.
        /// These are positions, not durability seqs — the
        /// controller back-fills seqs at append time (same
        /// convention as `CompactionRecordView`).
        shadowed_positions: Vec<u64>,
    },
    /// The lifecycle finished. `Committed` when the splice landed;
    /// `Failed` when the summariser errored, the summary was
    /// rejected as oversized, or the manager bailed before
    /// splicing.
    End {
        /// Correlation id.
        token: SurfaceToken,
        /// Terminal state.
        outcome: CompactionOutcome,
    },
}
