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
//!   methods live here.
//! - [`traits`]: The core types — [`traits::AgentContext`] (carried
//!   across a single LLM turn), the [`traits::AgentHook`] async
//!   trait (8 hook points), [`traits::ToolAction`] (3-variant enum
//!   for `Proceed` / `Skip` / `Modify`), and [`traits::ToolCall`]
//!   plus its `from_value` / `to_value` JSON conversion helpers.
//! - [`outcome`]: PR-4.1 `HookOutcome` 3-state + 10 typed events.
//! - [`hook_trait`]: PR-4.2 Unified `Hook` trait replacing the dual
//!   `AgentHook` + `HookRunner` system.
//! - [`loop_detector`]: PR-4.3 LoopDetector integration for repeated
//!   tool call detection.

pub mod dispatcher;
pub mod hook_trait;
pub mod loop_detector;
pub mod outcome;
pub mod registry;
pub mod traits;

// Re-export for backward compatibility.
// Re-export new unified types.
pub use dispatcher::UnifiedHookDispatcher;
pub use hook_trait::Hook;
pub use loop_detector::LoopDetector;
pub use outcome::{HookEvent, HookOutcome};
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
