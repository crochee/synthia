//! Prompt-section family.
//!
//! The `sections` module is the "what goes into the system
//! prompt" layer. Every concrete section implements the
//! [`PromptSection`] trait defined in [`section_trait`], and a
//! few text-formatting helpers shared across sections live in
//! [`helpers`].
//!
//! The 14 concrete section implementations each live in their
//! own sibling file (`agents_md` / `environment` / `identity` /
//! `language` / `mcp` / `memory` / `output_style` / `proactive`
//! / `skills` / `system` / `task_execution` / `team_mode` /
//! `token_budget` / `tools_usage`).
//!
//! The 31 unit tests live in [`tests`].
//!
//! # Caching contract
//!
//! Each section declares its
//! [`SectionCaching`](crate::prompt::SectionCaching) class:
//!
//! - `Cached` — identical across every call (system, identity,
//   task execution, tool usage).
//! - `SessionCached` — identical within a session but may
//!   change between sessions (memory, output style, language,
//   skills).
//! - `Volatile` — may change every call (environment, mcp
//!   instructions, token budget, proactive).
//!
//! The prompt builder uses this metadata for prefix-hash
//! tracking and KV-cache stability.

mod helpers;
mod section_trait;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

// Concrete section implementations.
pub mod agents_md;
pub mod environment;
pub mod identity;
pub mod language;
pub mod mcp;
pub mod memory;
pub mod output_style;
pub mod proactive;
pub mod skills;
pub mod system;
pub mod task_execution;
pub mod team_mode;
pub mod token_budget;
pub mod tools_usage;

// Trait + helpers re-exports (consumed via `super::PromptSection`
// and `super::prepend_bullets` from the sibling section files, and
// via `crate::prompt::PromptSection` from the prompt builder).
// Re-export the concrete section types so callers can
// `use crate::prompt::sections::SystemSection;` and friends.
pub use agents_md::AgentsMdSection;
pub use environment::EnvironmentSection;
pub use helpers::{
    inject_workspace_file,
    inject_workspace_files,
    join_lines,
    prepend_bullets,
};
pub use identity::IdentitySection;
pub use language::LanguageSection;
pub use mcp::DynamicMcpInstructionsSection;
pub use memory::MemorySection;
pub use output_style::OutputStyleSection;
pub use proactive::ProactiveSection;
pub use section_trait::PromptSection;
pub use skills::SkillSection;
pub use system::SystemSection;
pub use task_execution::TaskExecutionSection;
pub use team_mode::{TeamModeSection, TeamPromptInfo};
pub use token_budget::TokenBudgetSection;
pub use tools_usage::ToolUsageSection;
