//! Agent hook system.
//!
//! Defines [`traits::AgentContext`], the [`traits::AgentHook`] trait,
//! [`registry::HookRegistry`], and the type-conversion helpers for
//! working with strongly-typed [`synthia_provider::types::Message`]
//! and [`traits::ToolCall`] values.
//!
//! # Module Layout
//!
//! - [`registry`]: The [`registry::HookRegistry`] with panic isolation
//!   via `catch_unwind` and the [`registry::HookHandle`] returned by
//!   [`registry::HookRegistry::register_hook`]. All hook firing
//!   methods live here
//!   ([`registry::HookRegistry::fire_before_llm`],
//!   [`registry::HookRegistry::fire_after_llm`],
//!   [`registry::HookRegistry::fire_before_tool`],
//!   [`registry::HookRegistry::fire_after_tool`],
//!   [`registry::HookRegistry::fire_iteration_end`],
//!   [`registry::HookRegistry::fire_complete`]).
//! - [`traits`]: The core types — [`traits::AgentContext`] (carried
//!   across a single LLM turn), the [`traits::AgentHook`] async
//!   trait (8 hook points), [`traits::ToolAction`] (3-variant enum
//!   for `Proceed` / `Skip` / `Modify`), and [`traits::ToolCall`]
//!   plus its `from_value` / `to_value` JSON conversion helpers.
//! - `tests`: All 13 unit tests (in [`tests::basics`],
//!   [`tests::panic_recovery`], [`tests::modify`]).

pub mod registry;
pub mod traits;

// Re-export for backward compatibility.
pub use registry::{HookHandle, HookRegistry};
// Re-export from provider for convenience.
pub use synthia_provider::types::{Message, Role};
pub use traits::{
    AgentContext,
    AgentHook,
    ToolAction,
    ToolCall,
    message_from_value,
    message_to_value,
};

#[cfg(test)]
mod tests;
