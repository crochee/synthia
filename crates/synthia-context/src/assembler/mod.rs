//! `ContextAssembler` — layered context composition with
//! priority-driven trimming and a deterministic system-prompt
//! snapshot for prefix-cache tracking.
//!
//! Submodule layout:
//!
//! - [`types`]: the [`SectionPriorities`] struct + impl
//!   (priority overrides with resolver methods) and the
//!   [`ContextAssembler`] struct (private fields).
//! - [`construct`]: the constructor [`ContextAssembler::new`]
//!   and the 7 `with_*` builder methods
//!   (`with_system_prompt`, `with_protection_zone`,
//!   `with_token_counter`, `with_injector`,
//!   `with_injectors`, `with_workspace_info`,
//!   `with_priorities`).
//! - [`inject`]: the 8 `inject_*` / `set_*` / `add_*` state
//!   setters that mutate the assembler's buffers between
//!   build steps (`inject_memories`, `set_skill_summaries`,
//!   `set_skill_matches`, `add_active_skill_prompt`,
//!   `add_requested_snippets`, `add_tool_results`,
//!   `set_conversation`, `add_message`).
//! - [`assemble`]: the read-side methods
//!   ([`ContextAssembler::assemble_sections`],
//!   [`ContextAssembler::build_system_prompt`],
//!   [`ContextAssembler::section_by_name`],
//!   [`ContextAssembler::system_snapshot`]) plus the private
//!   `estimate_total_tokens` helper.
//! - [`trim`]: the free
//!   [`ContextAssembler::trim_to_budget`] function
//!   (operates on a `&mut Vec<Section>`; doesn't need
//!   `self`).
//! - [`pipeline`]: the two pipeline entry points
//!   ([`ContextAssembler::prepare`] and
//!   [`ContextAssembler::finalize`]) that drive the
//!   `MessageReader` -> [`CompletionRequest`] flow.
//!
//! Unit tests live in [`tests`].

mod assemble;
mod construct;
mod inject;
mod pipeline;
mod trim;
mod types;

#[cfg(test)]
mod tests;

pub use types::{ContextAssembler, SectionPriorities};
