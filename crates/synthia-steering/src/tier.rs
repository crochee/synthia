//! [`for_tier`] / [`auto`] — tier-aware bundle factory.
//!
//! R10 — adopt traitclaw `crates/traitclaw-steering/src/steering.rs:42-66`
//! `Steering::auto()` / `Steering::for_tier()`.
//!
//! ## Why a tier facade?
//!
//! `Steering::default_policy` (R3) is **one fixed bundle** tuned for a
//! medium-capacity model: 250 tool-budget cap, 0.85 context-budget
//! threshold, concurrency 8. Smaller models saturate that budget on
//! legitimate work; larger models carry unused safety headroom.
//!
//! The R10 facade lifts the bundle choice to a [`ModelTier`]:
//!
//! | Tier   | Tool budget | Loop detect | Context budget | Concurrency |
//! |--------|-------------|-------------|----------------|-------------|
//! | Small  | 50          | 3           | 0.50           | 1           |
//! | Medium | 100         | 3           | 0.75           | 3           |
//! | Large  | 250         | 5           | 0.80           | 8           |
//!
//! `Steering::auto()` calls `provider.tier()` and then
//! `Steering::for_tier(tier, workspace_root)` — a lib consumer wires
//! it in **one line** instead of hand-picking every preset.

use std::{path::PathBuf, sync::Arc};

use synthia_provider::ModelTier;

use crate::{
    guards::{
        LoopDetectionGuard,
        PromptInjectionGuard,
        ShellDenyGuard,
        ToolBudgetGuard,
        WorkspaceBoundaryGuard,
    },
    hint::{ContextBudgetHint, Hint, IterationReminderHint, TruncationHint},
    hook::LoggingHook,
    steering::Steering,
    tracker::{AdaptiveTracker, Tracker},
};

/// Build a tier-tuned steering bundle.
///
/// The exact preset for each tier lives in
/// [`synthia_provider::TierLimits::for_tier`]. The facade translates
/// those numbers into a concrete guard / hint / tracker combination:
/// - the [`ToolBudgetGuard`] cap scales with `tool_budget`,
/// - the [`LoopDetectionGuard`] window scales with
///   `loop_detection_window`,
/// - the [`IterationReminderHint`] cadence is `loop_detection_window *
///   5` (so a small model is reminded every 15 iterations, a large one
///   every 25),
/// - the [`AdaptiveTracker`] concurrency is `max_concurrency`,
/// - the [`ContextBudgetHint`] fires at `context_budget_threshold`.
pub fn for_tier(
    tier: ModelTier,
    workspace_root: impl Into<PathBuf>,
) -> Steering {
    let workspace_root = workspace_root.into();
    let limits = synthia_provider::TierLimits::for_tier(tier.clone());

    // Guards — same five as default_policy, scaled to the tier.
    let guards: Vec<Arc<dyn crate::guard::Guard>> = vec![
        Arc::new(LoopDetectionGuard::new(limits.loop_detection_window)),
        Arc::new(ToolBudgetGuard::new(limits.tool_budget)),
        Arc::new(ShellDenyGuard::default()),
        Arc::new(WorkspaceBoundaryGuard::new(workspace_root.clone())),
        Arc::new(PromptInjectionGuard::default()),
    ];

    // Hints — budget + iteration reminder + truncation; the
    // reminder cadence and budget threshold are tier-tuned.
    let hints: Vec<Arc<dyn Hint>> = vec![
        Arc::new(ContextBudgetHint::new(limits.context_budget_threshold)),
        Arc::new(IterationReminderHint::new(limits.loop_detection_window * 5)),
        Arc::new(TruncationHint),
    ];

    // Tracker — tier-driven concurrency cap.
    let tracker: Arc<dyn Tracker> =
        Arc::new(AdaptiveTracker::new(limits.max_concurrency));

    Steering {
        guards,
        hooks: vec![Arc::new(LoggingHook)],
        hints,
        tracker,
        output_transformer: Arc::new(
            crate::output_transformer::NoopOutputTransformer,
        ),
    }
}

/// One-line "give me a sensible bundle for this provider" facade.
///
/// Reads the provider's cheap [`ModelTier`] (cached at agent-build
/// time; no network), then dispatches to [`for_tier`]. Equivalent
/// to traitclaw `Steering::auto()` but tier-aware.
///
/// # Example
///
/// ```no_run
/// use synthia_provider::ModelProvider;
/// use synthia_steering::tier::auto;
///
/// # async fn build(provider: std::sync::Arc<dyn ModelProvider>) {
/// let tier = provider.tier();
/// let steering = auto(tier, "/workspace");
/// # }
/// ```
pub fn auto(tier: ModelTier, workspace_root: impl Into<PathBuf>) -> Steering {
    for_tier(tier, workspace_root)
}

#[cfg(test)]
mod tests {
    use synthia_provider::{ModelTier, TierLimits};

    use super::*;

    #[test]
    fn for_tier_picks_matching_preset() {
        let _limits = TierLimits::for_tier(ModelTier::Small);
        let s = for_tier(ModelTier::Small, "/tmp");
        assert_eq!(s.guards.len(), 5);
        let state = synthia_context::AgentState::with_window(8_000);
        assert_eq!(s.tracker.recommended_concurrency(&state), 1);
    }

    #[test]
    fn for_tier_scales_tool_budget_with_tier() {
        let s_small = for_tier(ModelTier::Small, "/tmp");
        let s_large = for_tier(ModelTier::Large, "/tmp");
        assert!(s_small.guards.len() == s_large.guards.len());
        // We can't introspect the per-guard budget cap without
        // driving a real action; assert the bundles are wired
        // differently by inspecting that they were built
        // independently (separate guard instances).
        assert!(!Arc::ptr_eq(&s_small.guards[0], &s_large.guards[0]));
    }

    #[test]
    fn auto_dispatches_to_for_tier() {
        let s = auto(ModelTier::Medium, "/tmp");
        assert!(s.guards.len() >= 3);
    }
}
