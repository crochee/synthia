//! Unified tool-output truncation service.
//!
//! This is the single LLM-context-time truncation point: any tool output that
//! would otherwise blow up the prompt window is preserved (head + tail) and
//! the full content is spilled to a per-call file under `cfg.temp_dir` for
//! later retrieval.
//!
//! It coexists with `tool_executor::truncate_result` for one release cycle;
//! that helper is an internal detail of the executor and is not replaced by
//! this module. Field-level backward compatibility with that helper is
//! preserved through `#[serde(alias)]` on `TruncatedResult`.
//!
//! Spec: `openspec/changes/streaming-2part-truncate/specs/tool-output-truncation/spec.md`
//!
//! Submodule layout:
//!
//! - [`types`]: the public [`TruncateConfig`] struct +
//!   `Default` impl, the public [`TruncatedResult`] struct,
//!   and the private `passthrough` constructor.
//! - [`truncate_output`]: the public
//!   [`truncate_output`](truncate_output::truncate_output) free
//!   function (the main single-string entry point) + the
//!   private `build_marker` helper that formats the
//!   `[... N bytes / M lines truncated ...]` marker line.
//! - [`truncate_messages`]: the public
//!   [`truncate_messages`](truncate_messages::truncate_messages)
//!   per-message variant (cleared-placeholder-aware + role
//!   predicate) + the public
//!   [`cleared_placeholder`](truncate_messages::cleared_placeholder)
//!   helper that formats the prune-idempotent-marker string.
//! - [`text`]: the three private content-mutator helpers used
//!   by [`truncate_messages`]: `replace_first_text_anywhere`
//!   (handles both Shape A `ToolResult` and Shape B `Text`
//!   content), `replace_first_in_tool_result`, and
//!   `set_msg_text`.
//! - [`lines`]: the two private line-shaping helpers used by
//!   [`truncate_output`]: `split_head_tail` and `cap_lines`.
//! - [`spill`]: the private `spill_to_disk` helper that
//!   writes the full content to a per-call file under
//!   `cfg.temp_dir`.
//!
//! Unit tests live in [`tests`].

mod cleanup;
mod lines;
mod spill;
mod text;
mod truncate_messages;
mod truncate_output;
mod types;

#[cfg(test)]
mod tests;

pub use cleanup::{
    DEFAULT_RETENTION,
    cleanup_tool_output_store,
    cleanup_tool_output_store_async,
};
pub use truncate_messages::{cleared_placeholder, truncate_messages};
pub use truncate_output::truncate_output;
pub use types::{TruncateConfig, TruncatedResult, default_tool_output_dir};
