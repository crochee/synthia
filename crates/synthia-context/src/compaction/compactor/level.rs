//! The [`CompactionLevel`] enum (L1 / L2 / L3) and its `as_usize`
//! method. Used by [`super::core::Compactor`] for routing and by
//! [`super::dispatch::auto_select_level`] for the per-budget-ratio
//! level recommendation.

/// Explicit compaction level enum for multi-level context compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionLevel {
    /// Level 1: LLM-powered summary generation
    Level1Summary,
    /// Level 2: Structured truncation (tool results → args + first line)
    Level2StructuredTruncation,
    /// Level 3: Marker-only retention (only `[call:name-completed]` markers)
    Level3MarkerOnly,
}

impl CompactionLevel {
    /// Returns the numeric level (1, 2, 3). Returns 0 for unknown.
    pub fn as_usize(&self) -> usize {
        match self {
            Self::Level1Summary => 1,
            Self::Level2StructuredTruncation => 2,
            Self::Level3MarkerOnly => 3,
        }
    }
}
