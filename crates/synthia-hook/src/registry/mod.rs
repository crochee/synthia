//! Hook registry — the in-memory store of registered
//! [`AgentHook`] implementations + their [`HookInfo`]
//! metadata + their `failed` set.
//!
//! # Module Layout
//!
//! - [`types`]: The 3 public data records
//!   ([`types::HookHandle`], [`types::HookFilter`],
//!   [`types::HookInfo`] + its `RegistryItem` impl).
//! - [`registry`]: The [`registry::HookRegistry`] struct +
//!   its `Default` impl + the 5 lifecycle methods
//!   (`new` / `register_hook` / `unregister_by_handle` /
//!   `len` / `is_empty` / `contains` / `is_failed` /
//!   `record_failure`).
//! - [`registry_trait`]: The `Registry<HookInfo>` trait
//!   impl (4 methods: `register` / `unregister` / `get` /
//!   `list`).
//! - [`fire`]: The 6 lifecycle-fire methods
//!   ([`fire::HookRegistry::fire_before_llm`] /
//!   `fire_after_llm` / `fire_before_tool` /
//!   `fire_after_tool` / `fire_iteration_end` /
//!   `fire_complete`) — each iterates over non-failed
//!   hooks and calls the corresponding `AgentHook` method
//!   through the `safe_hook_fail_open` wrapper.
//! - [`safety`]: The [`safety::safe_hook_fail_open`]
//!   wrapper that converts a panicking hook into a
//!   `FailPolicy`-appropriate default. + the
//!   [`safety::get_hook_name`] helper used by
//!   `register_hook` to populate `HookInfo::name`.
//! - [`tests`]: 7 unit tests covering the registry
//!   lifecycle (register / unregister_by_handle), the
//!   `Registry<HookInfo>` trait (`list` / `get` /
//!   `unregister` / `unregister` not-found).

mod fire;
#[allow(clippy::module_inception)]
mod registry;
mod registry_trait;
mod safety;
mod types;

#[cfg(test)]
mod tests;

pub use registry::HookRegistry;
pub use types::HookHandle;
