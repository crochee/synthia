//! The `Compactor` — the public, configurable entry point for
//! context compaction.
//!
//! Holds a configured level and two numeric limits (`max_input_length`,
//! `max_output_lines`). The actual per-level algorithms live in
//! [`super::super::level1`], [`super::super::level2`], and
//! [`super::super::level3`]; this struct is a thin facade that picks
//! the right level and converts its result into the [`CompactionPart`]
//! shape that callers (e.g. the orchestrator and the agent's stream
//! builder) write back to the session.
//!
//! # Module Layout
//!
//! - [`level`]: The [`level::CompactionLevel`] enum (L1 / L2 / L3)
//!   plus its `as_usize` method.
//! - [`core`]: The [`core::Compactor`] struct itself, plus
//!   `new` / `with_limits` constructors and the two private
//!   `estimate_token_count` / `estimate_tokens` helpers used by
//!   every level implementation.
//! - [`dispatch`]: The five public entry points:
//!   [`dispatch::compact`], [`dispatch::compact_with_provider`],
//!   [`dispatch::compact_with_marker`], [`dispatch::auto_select_level`],
//!   [`dispatch::compact_to_token_budget`]. They are pure dispatchers:
//!   they compute `original_tokens`, route to the right
//!   [`super::levels`] implementation, and wrap the result in
//!   [`CompactionPart`].
//! - [`levels`]: The four private per-level implementations
//!   ([`levels::level1_summary`], [`levels::level1_summary_with_provider`],
//!   [`levels::level2_truncate`], [`levels::level3_marker_only`])
//!   that do the actual work.
//! - [`tests`]: All 18 unit tests covering construction, all three
//!   levels, auto_select_level thresholds, compact_with_provider
//!   (LLM, none, failing, empty, anchor-truncation), and
//!   compact_to_token_budget (under-budget, oversized collapse).

mod core;
mod dispatch;
mod level;
mod levels;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use core::Compactor;

pub use level::CompactionLevel;
