//! Read-only sub-context views derived from an [`AgentRunConfig`].
//!
//! These types exist purely to give call sites a narrower API surface
//! than the full 20-field god-struct. They are zero-copy (all fields
//! are `Arc`/`&`-borrowed from the source config) and do not replace
//! `AgentRunConfig` — the existing entry point `Agent::run_stream`
//! still takes the full config. New code may opt into the narrower
//! views; legacy code is unchanged.
//!
//! Why three sub-contexts (rather than a single refactor)?
//!
//! - **Zero migration cost.** No call site is forced to change.
//! - **Bounded blast radius.** Each view re-exposes only the fields
//!   relevant to one concern (loop, persistence, orchestration).
//! - **Future-proofing.** When the eventual v4 collapses
//!   `AgentRunConfig` into the three sub-contexts, callers that
//!   already use the views need only swap the import — not rewrite
//!   their state-management code.
//!
//! # Examples
//!
//! ```ignore
//! use synthia_agent::config::agent_config::sub_contexts::LoopContext;
//!
//! fn tool_dispatch_loop(cfg: &AgentRunConfig) {
//!     let ctx = LoopContext::from(cfg);
//!     // ...use ctx.provider, ctx.model_router, ctx.session_id
//! }
//! ```

use std::sync::Arc;

use synthia_provider::{router::ModelRouter, traits::ModelProvider};
use synthia_session::Store as SessionStore;

use super::run_config::AgentRunConfig;
use crate::{
    control::{AgentControl, fork_policy::ForkPolicy},
    subagent::SubagentSessionFactory,
};

/// Read-only view over the fields needed to drive the inner LLM/tool loop.
///
/// Contains only the values that change between iterations of the
/// ReAct loop. Heavy, infrequent, or "set once at construction time"
/// state (tool registries, hook registries, sandbox, approval) is
/// deliberately excluded — pass the full `AgentRunConfig` for those.
#[allow(dead_code)] // public API fields; consumed by future call sites
#[derive(Clone)]
pub struct LoopContext<'a> {
    /// LLM provider used for every iteration.
    pub provider: &'a Arc<dyn ModelProvider>,
    /// Model router (fallback chain, capability-based selection).
    pub model_router: &'a Arc<ModelRouter>,
    /// Session identifier — used as KV-cache prefix anchor.
    pub session_id: &'a str,
    /// User namespace — gates prompt cache key and tool permissions.
    pub user_id: &'a str,
}

impl<'a> From<&'a AgentRunConfig> for LoopContext<'a> {
    fn from(cfg: &'a AgentRunConfig) -> Self {
        Self {
            provider: &cfg.provider,
            model_router: &cfg.model_router,
            session_id: &cfg.session_id,
            user_id: &cfg.user_id,
        }
    }
}

/// Read-only view over the fields needed to persist and resume sessions.
///
/// Contains only the persistence-related state. The `Store` itself
/// is `Clone` and Arc-backed internally, so deriving this view is
/// zero-copy — the clone shares the same `EventStore` seq cache.
#[allow(dead_code)] // public API fields; consumed by future call sites
#[derive(Clone)]
pub struct PersistenceContext<'a> {
    pub session_id: &'a str,
    pub user_id: &'a str,
    pub store: &'a SessionStore,
}

impl<'a> From<&'a AgentRunConfig> for PersistenceContext<'a> {
    fn from(cfg: &'a AgentRunConfig) -> Self {
        Self {
            session_id: &cfg.session_id,
            user_id: &cfg.user_id,
            store: &cfg.session_store,
        }
    }
}

/// Read-only view over the fields needed for multi-agent orchestration.
///
/// All fields here are `Option`-typed in the underlying config, so the
/// derived view preserves that nullable shape. When `agent_control` or
/// `subagent_session_factory` is `None`, the orchestrator must degrade
/// gracefully (single-agent mode) — that contract is documented at
/// `AgentControl::new`.
#[allow(dead_code)] // public API fields; consumed by future call sites
#[derive(Clone)]
pub struct OrchestrationContext<'a> {
    pub agent_control: Option<&'a AgentControl>,
    pub subagent_session_factory: Option<&'a Arc<dyn SubagentSessionFactory>>,
    pub fork_policy: &'a ForkPolicy,
}

impl<'a> From<&'a AgentRunConfig> for OrchestrationContext<'a> {
    fn from(cfg: &'a AgentRunConfig) -> Self {
        Self {
            agent_control: cfg.agent_control.as_ref(),
            subagent_session_factory: cfg.subagent_session_factory.as_ref(),
            fork_policy: &cfg.fork_policy,
        }
    }
}
