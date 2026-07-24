//! Subagent execution module.
//!
//! Provides the [`SubagentSessionFactory`] trait so agent-side code can
//! create real child sessions through the server's session lifecycle.
//!
//! Sub-modules:
//! - [`config`]: builds a child [`AgentRunStateConfig`] from the parent
//!   runtime configuration, including message history filtering and permission
//!   wiring.
//! - [`factory`]: defines [`SubagentSessionFactory`] and related types for
//!   launching child sessions.
//! - [`guardian_bridge`]: adapts [`SubagentSessionFactory`] to the
//!   guardian-local [`GuardianSubagentFactory`] trait (pure type
//!   conversion).
//! - [`permission`]: derives permission rules scoped to a sub-agent from the
//!   parent's policy and the requested tool set.

pub mod config;
pub mod factory;
pub mod guardian_bridge;
pub mod permission;

pub use config::build_subagent_config;
pub use factory::{
    ChildSessionHandle,
    SubagentSessionError,
    SubagentSessionFactory,
    truncate_summary,
};
pub use guardian_bridge::GuardianSubagentFactoryBridge;
pub use permission::derive_subagent_permission;
