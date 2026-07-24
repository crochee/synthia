//! `CompactionResult` write-back shape for the orchestrator.

use crate::types::SummaryMessage;

/// Result of a compaction operation, suitable for write-back to session.
///
/// The orchestrator's write-back shape. Distinct from
/// `crate::compaction_service::CompactionResult`, which is a
/// smaller (old/new token counts only) value used by the
/// `compact_messages` helper — that helper is a higher-level
/// convenience built on top of `Compactor` and does not need the
/// indices / applied_level / summary detail.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Indices of original messages that were compacted.
    pub compacted_indices: Vec<usize>,
    /// The compaction level that was applied (1, 2, or 3).
    pub applied_level: usize,
    /// The summary message to write back.
    pub summary: SummaryMessage,
    /// Token savings from compaction.
    pub original_tokens: usize,
    pub compacted_tokens: usize,
}

impl CompactionResult {
    pub fn savings(&self) -> usize {
        self.original_tokens.saturating_sub(self.compacted_tokens)
    }
}
