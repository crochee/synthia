//! Hierarchical discovery of `AGENTS.md` files for the system prompt.
//!
//! Walks `workspace_dir`'s ancestor directories from deepest to root,
//! collecting every file whose name matches the configured `filenames`
//! list (default: `["AGENTS.md"]`). The collected files are then
//! concatenated **farthest → closest** so the most-specific file appears
//! last and can override global conventions in the LLM's reading order.
//!
//! The section is cached as [`SectionCaching::SessionCached`] so file
//! edits between sessions are picked up but the read is reused across
//! LLM calls within a session.
//!
//! See: `openspec/changes/agents-md-hierarchical-discovery/specs/agents-md-hierarchical-discovery/spec.md`.
//!
//! # Module Layout
//!
//! - [`config`]: The [`config::AgentsMdConfig`] struct (the master
//!   switch, filename list, per-file and total character caps) plus
//!   the three default constants ([`config::DEFAULT_MAX_CHARS_PER_FILE`],
//!   [`config::DEFAULT_MAX_CHARS_TOTAL`], [`config::DEFAULT_FILENAME`]).
//!   Also owns the private [`config::DiscoveredFile`] struct that the
//!   walk/merge pipeline produces.
//! - [`section`]: The [`section::AgentsMdSection`] struct itself, plus
//!   its public constructors ([`section::AgentsMdSection::new`],
//!   [`section::AgentsMdSection::with_config`],
//!   [`section::AgentsMdSection::config`]) and the [`PromptSection`]
//!   impl that wires the walk → merge → format pipeline together.
//! - [`walk`]: [`walk::walk_ancestors`] — the filesystem walk that
//!   produces a `Vec<DiscoveredFile>` in **farthest-to-closest** order.
//!   Handles symlink-cycle detection via canonical paths and silently
//!   skips files that fail to read.
//! - [`pipeline`]: The output-shaping pipeline:
//!   [`pipeline::merge_within_limit`] (per-file + total truncation),
//!   [`pipeline::truncate_with_marker`] (single-file helper), and
//!   [`pipeline::format_merged`] (the final `<agents_md>`-wrapped
//!   string).
//! - [`tests`]: All 20 unit tests covering config, truncate_with_marker,
//!   walk_ancestors (including symlink cycle), merge_within_limit,
//!   and the [`PromptSection`] impl's `name` / `caching` / `build`.

mod config;
mod pipeline;
mod section;
mod walk;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use config::{
    AgentsMdConfig,
    DEFAULT_FILENAME,
    DEFAULT_MAX_CHARS_PER_FILE,
    DEFAULT_MAX_CHARS_TOTAL,
};
pub use section::AgentsMdSection;
