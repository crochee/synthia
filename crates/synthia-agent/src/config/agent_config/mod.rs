//! Agent configuration types and builders.
//!
//! The `agent_config` module owns every static and runtime configuration
//! value the agent needs:
//!
//! - **Static config** ([`AgentConfig`], [`AgentConfigBuilder`]):
//!   serialised to disk via TOML/JSON, carries model, max_tokens, max_iterations,
//!   workspace_root, observability, executor kind, AGENTS.md discovery knobs.
//! - **Runtime config** ([`AgentRunConfig`], [`AgentRunConfigBuilder`]):
//!   in-memory only, carries the model provider, tool/hook registries,
//!   session store, cancel token, the [`user_id`](AgentRunConfig::user_id)
//!   namespace (see `user-id-namespace-and-bash-permission-gate` OpenSpec
//!   change), and optional multi-agent / fork-policy / L4-compaction
//!   channels.
//! - **Misc**: [`ObservabilityConfigInner`] (serde shape of
//!   `AgentConfig::observability`), [`AgentImplementation`] /
//!   [`ExecutorKindConfig`] (serde enums), [`AgentRunStateConfig`]
//!   (the frozen runtime snapshot used for resume / fork).
//!
//! # Module Layout
//!
//! - [`observability`]: [`ObservabilityConfigInner`] + the
//!   `From<&ObservabilityConfigInner> for ObservabilityConfig` bridge.
//! - [`enums`]: [`AgentImplementation`] and [`ExecutorKindConfig`]
//!   serde enums (used by [`AgentConfig`] and the executor selector).
//! - [`agent_config`]: [`AgentConfig`] struct, its `Default` impl, the
//!   `validate()` invariant check (max_iterations > 0, token-budget
//!   thresholds monotonic), and the `agents_md_config()` bridge to
//!   `synthia_context::prompt::sections::agents_md::AgentsMdConfig`.
//! - [`agent_config_builder`]: [`AgentConfigBuilder`] with 11 setter
//!   methods + `build()` (falls back to [`AgentConfig::default`] for
//!   unset fields, then runs `validate()`).
//! - [`run_config`]: [`AgentRunConfig`] + [`AgentRunStateConfig`]
//!   data structs.
//! - [`run_config_builder`]: [`AgentRunConfigBuilder`] with 15 setter
//!   methods + `build()` (enforces `user_id` non-empty as a hard
//!   validation rule).
//! - [`tests`]: All 13 unit tests covering validation, defaults,
//!   serde backward-compat, and the agents_md_config bridge.

#[allow(clippy::module_inception)]
mod agent_config;
mod agent_config_builder;
mod enums;
mod observability;
mod run_config;
mod run_config_builder;
pub mod sub_contexts;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod sub_contexts_tests;
#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use agent_config::AgentConfig;
pub use agent_config_builder::AgentConfigBuilder;
pub use enums::{AgentImplementation, ExecutorKindConfig};
pub use observability::ObservabilityConfigInner;
pub use run_config::{AgentRunConfig, AgentRunStateConfig};
pub use run_config_builder::AgentRunConfigBuilder;
