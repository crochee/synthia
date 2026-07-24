//! Tool pruning strategies with importance-based differentiation.
//!
//! Submodules:
//! - [`classify`]: `MessageClassification` + tool-pair repair + `micro_compact`
//! - [`stages`]: progressive-degradation primitives
//!   (`PruningStage`, `EventType`, `PruningConfig`, soft/hard trim, level map)
//! - [`engine`]: `PruneStats` + the main `prune()` entry point

mod classify;
mod engine;
mod stages;

pub use classify::{
    MessageClassification,
    classify_messages,
    find_result_for_tool_use,
    find_tool_use_for_result,
    fix_tool_pairs,
    get_tool_result_id,
    get_tool_use_id,
    is_tool_result,
    is_tool_use,
    is_user_text_message,
    micro_compact,
};
pub use engine::{PRUNE_PROTECT_TOKENS, PruneStats, prune};
pub use stages::{
    EventType,
    PruningConfig,
    PruningStage,
    get_compression_level,
    hard_clear_content,
    soft_trim_content,
};
