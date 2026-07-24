//! Tool catalog + dispatch.
//!
//! The original 769-line `registration.rs` was split
//! into focused submodules by responsibility:
//!
//! - [`entry`]: the [`entry::ToolEntry`] value type
//!   (one entry per registered tool) plus its
//!   `RegistryItem` / `Serialize` / `Deserialize`
//!   impls.
//! - [`registry`]: the [`registry::ToolRegistry`]
//!   struct, its inherent surface
//!   ([`registry::ToolRegistry::new`],
//!   [`registry::ToolRegistry::register`],
//!   [`registry::ToolRegistry::run_with_context`],
//!   [`registry::ToolRegistry::execute_tools`], and
//!   the `with_*` builders), and its `Clone` impl.
//! - [`registry_trait`]: the `impl
//!   ` [`Registry<ToolEntry>`] for
//!   [`registry::ToolRegistry`] block — kept
//!   separate so the inherent API and the trait
//!   surface can evolve independently.
//!
//! The 12 unit tests live in [`tests`].
//!
//! This layout mirrors the other three
//! `*registry`/`*manager` modules in the workspace
//! (`synthia-agent/registry/`,
//! `synthia-skill/registry/`, `synthia-mcp/manager/`),
//! all of which follow the
//! `types` / `*_registry` / `registry_trait` / `tests`
//! shape.

mod entry;
mod registry;
mod registry_trait;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use entry::ToolEntry;
pub use registry::ToolRegistry;
