//! Context injection system for extending the context assembly with pluggable data sources.
//!
//! Provides the `ContextInjector` trait for injecting system prompts and memories
//! from external sources during context assembly, and the `Section` struct for
//! priority-based context trimming.

pub mod priorities;
mod section;
mod skill;
mod r#trait;

#[cfg(test)]
mod tests;

pub use section::Section;
pub use skill::SkillInjector;
pub use r#trait::ContextInjector;
