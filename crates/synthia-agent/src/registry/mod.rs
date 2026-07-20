//! Agent registry — definitions, instances, and the
//! `Registry<AgentDefinition>` trait impl.
//!
//! Submodule layout:
//!
//! - [`agent_registry`]: the [`AgentRegistry`] struct, its
//!   `new` / `with_*` / `Default` / `Clone` impls.
//! - [`load`]: filesystem-driven agent definition loading
//!   ([`AgentRegistry::load_from_path`] + the private
//!   `load_definition_from_dir`).
//! - [`instances`]: instance lifecycle — `spawn`,
//!   `instance_exists`, `stop`, `stop_tree`, `wrap_as_tool`,
//!   `list_instances`, `instance_count`.
//! - [`query`]: definition-level read queries —
//!   `filter_definitions` (used by `Registry::list`),
//!   `contains`, `len`, `is_empty`.
//! - [`registry_trait`]: `RegistryItem for AgentDefinition` +
//!   `Registry<AgentDefinition> for AgentRegistry`.
//!
//! Sibling modules (kept as separate files because they
//! predate the split):
//!
//! - [`instance`]: the [`AgentStatus`] enum +
//!   [`AgentResult`] / [`AgentTokenUsage`] types.
//! - [`permission_builder`]: agent permission policy
//!   building (allowed tools, merged policy).
//! - [`tool_wrapper`]: the [`AgentToolWrapper`] adapter.
//! - [`types`]: [`AgentDefinition`] + [`AgentFilter`] data
//!   types.
//!
//! Unit tests live in [`tests`].

mod agent_registry;
mod instances;
mod load;
mod query;
mod registry_trait;
#[cfg(test)]
mod tests;

pub mod instance;
pub mod permission_builder;
pub mod tool_wrapper;
pub mod types;

pub use agent_registry::AgentRegistry;
pub use instance::{AgentResult, AgentStatus, AgentTokenUsage};
pub use permission_builder::{
    build_allowed_tools,
    build_merged_policy,
    is_tool_allowed,
};
pub use synthia_task::types::{StructuredOutput, Task, TaskStatus};
pub use tool_wrapper::AgentToolWrapper;
pub use types::{AgentDefinition, AgentFilter};
