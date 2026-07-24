//! The [`PromptBuilder`] itself: assembles static + dynamic halves of
//! a system prompt from a list of [`PromptSection`]s, with prefix-hash
//! tracking for KV-cache stability.
//!
//! # Module Layout
//!
//! - [`core`]: The [`core::PromptBuilder`] struct itself, plus
//!   `new` / `add_section` / `section_names` / `get_static_sections` /
//!   `get_dynamic_sections` / `Debug` / `Default` impls.
//! - [`defaults`]: The two builder presets:
//!   [`defaults::default_with_sections`] (all 13 sections) and
//!   [`defaults::build_for_name`] (variant that picks the right
//!   `TeamModeSection` based on the agent's role).
//! - [`resolve`]: The section-iteration core ([`resolve::resolve`])
//!   plus the KV-cache stability check
//!   ([`resolve::validate_prefix_stability`]). Both rely on
//!   `DefaultHasher` for the `prefix_hash` / `static_hash` outputs.
//! - [`effective`]: [`effective::build_effective_prompt`] — the
//!   final-assembly step that applies the
//!   [`EffectivePromptConfig`](super::config::EffectivePromptConfig)
//!   overrides (override_prompt, coordinator_mode, prompt, agent_prompt,
//!   custom_prompt, append_prompt) on top of the resolved prompt.
//! - [`tests`]: All 19 unit tests covering construction, resolve
//!   (per caching kind), validate_prefix_stability, getters, and
//!   build_effective_prompt override paths.

mod core;
mod defaults;
mod effective;
mod resolve;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use core::PromptBuilder;
