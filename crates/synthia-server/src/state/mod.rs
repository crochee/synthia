//! Shared server state container — [`AppState`] and friends.
//!
//! ## Layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `app_state` | The `AppState` struct + accessors; its `boot` / `for_test` / `sessions` / `usage` / `tools` submodules hold the constructors, the controller cache, the usage counters, and the `AppliedToolSurface` record of what `[tools]` did. |
//! | `boot`      | The boot-time helpers (`load_default_*`, `register_configured_mcp_servers`, `apply_tools_config`, `build_prompt_context`, …) — one free function per concern, called in order from `AppState::with_server_config`. |
//! | `tests`     | The test suite, split by concern into one file per surface (registry fallback, prompt assembler, surface application, MCP boot, …). |
//!
//! ## Boot helpers
//!
//! The boot helpers are `pub(crate)` — they are internal
//! steps of the boot the public surface never names. The
//! `pub(crate) use` re-exports below give the `tests/`
//! submodules a single path to import them from
//! (`crate::state::xxx`) without leaking them outside the
//! crate.

mod app_state;
mod boot;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use app_state::plugin_tool_registry;
pub use app_state::{
    AppState,
    AppliedToolSurface,
    ModelSelectionError,
    ProviderCatalogue,
    UsageMetrics,
    UsageSnapshot,
};

/// Active session controllers keyed by `(user_id, session_id)`.
///
/// A type alias (not a newtype): every use site wants the map's own
/// API, and the key's meaning is fixed by [`AppState`]'s
/// field of this type.
pub type ActiveSessions = dashmap::DashMap<
    (String, String),
    std::sync::Arc<crate::session::controller::SessionController>,
>;
#[cfg(test)]
pub(crate) use boot::{
    apply_tools_config,
    build_prompt_context,
    load_default_compaction,
    load_default_max_iterations,
    load_default_strategy,
    load_default_tool_restriction,
    register_configured_mcp_servers,
    unapplied_agent_keys,
};
