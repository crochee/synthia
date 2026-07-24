//! Level 1: LLM-summary compaction.
//!
//! This is the highest-fidelity compaction level: it asks an LLM
//! (via the `CompactionProvider` trait) to write a structured
//! summary of the conversation. When the provider is unavailable,
//! fails, or returns an empty summary, the structured-fallback
//! path renders a heuristic summary that captures the same
//! information shape (decisions / tools-used / findings) using only
//! the message text and a `<previous-summary>` anchor block for
//! decision continuity across successive L1 compactions.

mod compact;
mod fallback;
mod helpers;
mod provider;

#[cfg(test)]
mod tests;

pub use compact::compact_level1;
pub(crate) use fallback::build_structured_summary_fallback;
pub use fallback::{PREVIOUS_SUMMARY_HEAD_RATIO, PREVIOUS_SUMMARY_MAX_CHARS};
pub use provider::CompactionProvider;
