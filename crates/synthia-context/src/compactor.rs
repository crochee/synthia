//! Legacy re-export - all code importing from crate::compactor still works.
//!
//! The historical single-file `compactor.rs` was split into
//! `compaction/{compactor, level1, level2, level3, orchestrator, util}`
//! for navigability; this shim preserves the previous public surface
//! (`crate::compactor::*`) so downstream callers (agent, server, e2e
//! tests, ...) keep compiling.
pub use crate::compaction::{
    compactor::*,
    level1::{CompactionProvider, compact_level1},
    level2::compact_level2,
    level3::compact_level3,
    orchestrator::{
        CompactionResult,
        apply_compaction,
        calculate_protection_zone,
        compact_with_fallback,
    },
};
