//! Compaction orchestrator.
//!
//! The "orchestrator" is the entry-point layer: it owns the
//! L1 → L2 → L3 fallback chain (`apply_compaction` and
//! `compact_with_fallback`), the `CompactionResult` write-back
//! shape, the `SummaryMessage::from_compaction` factory, and the
//! `calculate_protection_zone` algorithm that decides which range
//! of messages is safe to compact. It does *not* know how to
//! compact any specific level — it delegates to the `level1` /
//! `level2` / `level3` sub-modules.
//!
//! Token-estimate duplication note: the orchestrator also runs a
//! single-pass `estimate_tokens` for `original_tokens` reporting
//! (project memory: "Compaction logic must use single-pass scanning
//! to eliminate O(n²) performance issues"). The per-level helpers
//! have their own `estimate_tokens` calls, but those are on a
//! different message set (the compacted output), so the O(n²)
//! pathology the project memory warns about does not reappear.

mod fallback;
mod protection;
mod result;

#[cfg(test)]
mod tests;

pub use fallback::{apply_compaction, compact_with_fallback};
pub use protection::calculate_protection_zone;
pub use result::CompactionResult;
