//! `apply_patch` — Anthropic V4A format multi-file edits.
//!
//! Operations apply sequentially in source order. If a later operation fails,
//! earlier successful operations are intentionally retained (consistent with
//! codex scenario 015 and opencode's explicit "atomic rollback are not
//! supported yet" stance). The tool returns a structured `AppliedFailure`
//! reporting both the applied and the failed operations so the LLM can
//! re-plan.
//!
//! See `v4a.rs` for the parser and `specs/apply-patch-tool/spec.md` for the
//! formal requirements.
//!
//! # Module Layout
//!
//! - [`input`]: The [`input::ApplyPatchInput`] deserializer.
//! - [`op_summary`]: The [`op_summary::op_summary`] helper that
//!   formats one [`v4a::PatchOp`] as `A path` / `M path` / `D path`
//!   for tool output.
//! - [`apply`]: The actual file mutation pipeline
//!   ([`apply::apply_one`] dispatches on `PatchOp` variant;
//!   [`apply::apply_hunks`] sequentially applies a list of
//!   [`v4a::Hunk`]s to file content; [`apply::find_hunk`] locates
//!   a hunk's `old_text` in the file, with a no-trailing-newline
//!   fallback for files that lack one).
//! - [`tool`]: The [`tool::ApplyPatchTool`] struct itself and its
//!   [`crate::traits::Tool`] impl. The `call` method is organized
//!   in 5 stages (parse → reject Move → resolve → sequential apply
//!   → summarize).
//! - [`tests`]: All 12 unit tests covering each `PatchOp` variant
//!   alone, multiple ops in one patch, partial-failure retention,
//!   Move rejection, path-traversal blocking, empty-patch
//!   rejection, registration in the default registry, the
//!   permission + concurrency-safe flags, overwrite semantics for
//!   `*** Add File:`, and directory-deletion blocking.

mod apply;
mod input;
mod op_summary;
mod tool;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use input::ApplyPatchInput;
pub use tool::ApplyPatchTool;
