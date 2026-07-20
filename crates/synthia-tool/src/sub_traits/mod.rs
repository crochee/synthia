//! Tool sub-traits: decomposing the monolithic [`crate::Tool`] trait.
//!
//! The legacy `Tool` trait has 12+ methods spanning three concerns:
//!
//! 1. **Definition** (what am I?) — name, description, schema, category
//! 2. **Execution** (what do I do?) — call, validate, dry-run, cost, cancel
//! 3. **Lifecycle** (am I alive?) — register, unregister, health, version
//!
//! This module provides three focused sub-traits, each ≤ 5 methods,
//! that can be composed via the [`ToolV1`] supertrait for backward
//! compatibility.
//!
//! The [`bridge`] submodule provides blanket implementations so that any
//! type implementing the legacy [`crate::Tool`] trait automatically
//! satisfies all three sub-traits.

pub mod bridge;
pub mod category;
pub mod definition;
pub mod execution;
pub mod lifecycle;

pub use category::ToolCategory;
pub use definition::{ToolDefinition, ToolMetadataSnapshot};
pub use execution::ToolExecution;
pub use lifecycle::ToolLifecycle;
