//! The 3 struct definitions.
//!
//! No business logic — just shapes. The factory methods
//! ([`StreamBuilder::from_config`], [`BuilderSteps::new`])
//! live in [`super::construct`]; the builder-pattern
//! accessors live in [`super::setters`].
//!
//! Field visibility is `pub(super)` throughout so the
//! sibling submodules can read/write them but the
//! external API surface (just [`StreamBuilder`]) is
//! unchanged.

use std::sync::Arc;

use parking_lot::Mutex;
use synthia_context::{
    assembler::ContextAssembler,
    prefix_tracker::{PrefixStabilityEvent, PrefixTracker},
};
use synthia_provider::types::Message;

use crate::{
    error_recovery::{
        ConsecutiveFailureTracker,
        ErrorRecoveryCoordinator,
        ResetCoordinator,
    },
    steering::SteeringChannel,
    stream_builder::{
        hook_builder::HookBuilder,
        steps::{StepCompact, StepReflect, StepSample, StepToolExecute},
    },
    types::AgentConfig,
};

/// Simple builder for running agent sessions with a given config.
///
/// Currently a thin wrapper around [`AgentConfig`] reserved
/// for future richer builder methods. The full streaming
/// surface lives on [`StreamBuilder`].
pub struct AgentBuilder {
    pub(super) _config: AgentConfig,
}

/// Streaming agent session builder.
///
/// Holds the session-scoped context — the
/// [`ContextAssembler`] and [`HookBuilder`] — plus the
/// cross-iteration state: the [`PrefixTracker`] and the
/// optional prefix-event callback.
///
/// The per-iteration [`BuilderSteps`] are allocated fresh
/// inside [`StreamBuilder::run`] so each session gets its own
/// failure tracker, reset coordinator, and steering channel
/// binding.
pub struct StreamBuilder {
    pub(super) context: ContextAssembler,
    pub(super) hooks: HookBuilder,
    pub(super) initial_state: Option<(Vec<Message>, usize)>,
    /// KV-Cache prefix tracker — shared across iterations, defaults to
    /// a fresh tracker. Can be replaced via `with_prefix_tracker` for
    /// multi-session aggregation.
    pub(super) prefix_tracker: Arc<Mutex<PrefixTracker>>,
    /// Optional callback invoked with the `PrefixStabilityEvent` after
    /// each LLM call. Used by telemetry to surface cache hit rate.
    pub(super) on_prefix_event:
        Option<Arc<dyn Fn(PrefixStabilityEvent) + Send + Sync>>,
}

/// Per-iteration helpers allocated fresh for each
/// [`StreamBuilder::run`] call.
///
/// Lives for the lifetime of one `run()` invocation. The
/// cascade-owned state ([`BuilderSteps::reset`] and
/// [`BuilderSteps::failure_tracker`]) is intentionally
/// per-session rather than per-`StreamBuilder` so that
/// multiple sequential runs of the same builder do not
/// leak cooldown windows / failure counts across sessions.
pub struct BuilderSteps {
    pub(super) sample: StepSample,
    pub(super) tool_execute: StepToolExecute,
    pub(super) compact: StepCompact,
    pub(super) reflect: StepReflect,
    pub(super) hooks: HookBuilder,
    /// Unified hook dispatcher — the single dispatch point for
    /// all hook events. Replaces the deprecated `HookBuilder::fire_*`
    /// methods. Constructed from `HookRegistry` hooks wrapped in
    /// `AgentHookAdapter`, plus any additional `Hook` trait impls
    /// (e.g. `LoopDetector`).
    pub(super) hook_dispatcher: Arc<synthia_hook::UnifiedHookDispatcher>,
    pub(super) recovery: ErrorRecoveryCoordinator,
    /// L5 reset coordinator — tracks the cooldown window that follows a
    /// failed reset, and owns the `ResetScope::Conversation` logic used
    /// by the recovery cascade. Fresh per session (see `BuilderSteps::new`).
    pub(super) reset: ResetCoordinator,
    /// Per-tool consecutive-failure counter. L3 reads this to decide
    /// whether the `FallbackProvider` has been hit twice in a row for
    /// the same tool, and the cascade clears it on every successful
    /// `Recovered` outcome. Fresh per session.
    pub(super) failure_tracker: ConsecutiveFailureTracker,
    pub(super) steering_channel: Option<Arc<dyn SteeringChannel>>,
}
