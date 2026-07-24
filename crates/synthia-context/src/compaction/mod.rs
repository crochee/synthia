//! Compaction module - context compression and summarization
//!
//! This module provides three-level degradation compaction:
//! - Level 1: LLM summary generation
//! - Level 2: Structured truncation
//! - Level 3: Marker-only retention

pub mod compactor;
pub mod level1;
pub mod level2;
pub mod level3;
pub mod orchestrator;
mod util;

#[cfg(test)]
mod test_providers;

// Re-export main types for convenience
pub use compactor::{CompactionLevel, Compactor};
pub use level1::{CompactionProvider, compact_level1};
pub use level2::compact_level2;
pub use level3::compact_level3;
pub use orchestrator::{
    CompactionResult,
    apply_compaction,
    calculate_protection_zone,
    compact_with_fallback,
};
