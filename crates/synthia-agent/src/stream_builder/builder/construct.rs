//! Factory methods — [`StreamBuilder::from_config`] and
//! [`BuilderSteps::new`].
//!
//! [`StreamBuilder::from_config`] allocates the
//! session-scoped context ([`ContextAssembler`] +
//! [`HookBuilder`]) from an [`AgentRunConfig`]. The
//! [`ContextAssembler`] is initialised with the
//! `context_token_budget.hard_limit` when set, falling
//! back to 4096.
//!
//! [`BuilderSteps::new`] allocates the per-iteration
//! helpers from an [`AgentRunConfig`] and the inherited
//! [`HookBuilder`]. Cascade-owned state (the
//! [`ResetCoordinator`] and the
//! [`ConsecutiveFailureTracker`]) is freshly constructed
//! so that sequential `run()` calls on the same builder
//! do not leak state across sessions.

use std::sync::Arc;

use parking_lot::Mutex;
use synthia_context::{
    assembler::ContextAssembler,
    prefix_tracker::PrefixTracker,
};

use crate::{
    config::AgentRunConfig,
    error_recovery::{
        ConsecutiveFailureTracker,
        ErrorRecoveryCoordinator,
        ResetCoordinator,
    },
    stream_builder::{
        builder::types::{BuilderSteps, StreamBuilder},
        hook_builder::HookBuilder,
        steps::{StepCompact, StepReflect, StepSample, StepToolExecute},
    },
};

impl StreamBuilder {
    /// Build a [`StreamBuilder`] from an [`AgentRunConfig`].
    ///
    /// The context token budget defaults to 4096 tokens when
    /// `AgentRunConfig::config.context_token_budget` is unset —
    /// the upstream `ContextAssembler` uses the same fallback
    /// and tests rely on it.
    pub fn from_config(config: &AgentRunConfig) -> Self {
        let max_tokens = config
            .config
            .context_token_budget
            .as_ref()
            .map(|b| b.hard_limit)
            .unwrap_or(4096);
        let context = ContextAssembler::new(max_tokens);

        let hooks = HookBuilder::new(config.hook_registry.clone());

        Self {
            context,
            hooks,
            initial_state: None,
            prefix_tracker: Arc::new(Mutex::new(PrefixTracker::new())),
            on_prefix_event: None,
        }
    }
}

impl BuilderSteps {
    /// Allocate a fresh per-session `BuilderSteps`.
    ///
    /// Cascade-owned state ([`ResetCoordinator`] +
    /// [`ConsecutiveFailureTracker`]) is freshly constructed
    /// here rather than reused from the [`StreamBuilder`]
    /// because the cascade's L3 (fallback) and L5 (reset)
    /// arms accumulate state that should not leak across
    /// sessions.
    pub(super) fn new(config: &AgentRunConfig, hooks: HookBuilder) -> Self {
        // Construct the UnifiedHookDispatcher. If LoopServices has already
        // bootstrapped a dispatcher, reuse it; otherwise construct a fresh
        // one from the HookRegistry.
        let hook_dispatcher = config
            .loop_services
            .get()
            .map(|ls| ls.hook_dispatcher.clone())
            .unwrap_or_else(|| {
                let mut dispatcher =
                    synthia_hook::UnifiedHookDispatcher::from_hook_registry(
                        hooks.get_registry(),
                    );
                dispatcher
                    .add_hook(Arc::new(synthia_hook::LoopDetector::new()));
                Arc::new(dispatcher)
            });

        Self {
            sample: StepSample::new(config.config.clone()),
            tool_execute: StepToolExecute::new(config),
            compact: StepCompact,
            reflect: StepReflect::new(config.config.model.clone()),
            hooks,
            hook_dispatcher,
            recovery: ErrorRecoveryCoordinator::new(5),
            reset: ResetCoordinator::new(),
            failure_tracker: ConsecutiveFailureTracker::new(),
            // Registry-First: prefer steering_channel from
            // InterceptorChain when available; fall back to
            // run_config field for legacy path.
            steering_channel: config
                .interceptor_chain
                .as_ref()
                .and_then(|c| c.steering_channel().cloned())
                .or_else(|| config.steering_channel.clone()),
        }
    }
}
