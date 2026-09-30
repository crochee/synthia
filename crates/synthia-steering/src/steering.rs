//! [`Steering`] — one bundle holding every steering component an
//! agent run consumes.
//!
//! The agent holds a single `Arc<Steering>` and consults it at
//! the documented boundaries; `Steering::noop()` is the neutral
//! bundle (behaviour identical to a pre-steering agent) and
//! `Steering::default_policy` is the production bundle wired by
//! the server.

use std::{path::PathBuf, sync::Arc};

use crate::{
    guard::Guard,
    guards::{
        LoopDetectionGuard,
        PromptInjectionGuard,
        ShellDenyGuard,
        ToolBudgetGuard,
        WorkspaceBoundaryGuard,
    },
    hint::{ContextBudgetHint, Hint, IterationReminderHint, TruncationHint},
    hook::{AgentHook, LoggingHook},
    output_transformer::{NoopOutputTransformer, OutputTransformer},
    tracker::{AdaptiveTracker, NoopTracker, Tracker},
};

/// The complete steering configuration for an agent.
#[derive(Clone)]
pub struct Steering {
    /// Pre-execution policy pipeline (sync, fail-closed).
    pub guards: Vec<Arc<dyn Guard>>,
    /// Async lifecycle observers (in registration order).
    pub hooks: Vec<Arc<dyn AgentHook>>,
    /// Advisory context injections consulted before each LLM
    /// call.
    pub hints: Vec<Arc<dyn Hint>>,
    /// Run-progress observer + concurrency recommendation.
    pub tracker: Arc<dyn Tracker>,
    /// Tool-output post-processor (identity by default).
    pub output_transformer: Arc<dyn OutputTransformer>,
}

impl Steering {
    /// Neutral bundle: no guards, no hooks, no hints, unlimited
    /// concurrency, identity transforms. Behaviour is identical
    /// to an agent built before the steering layer existed.
    pub fn noop() -> Self {
        Self {
            guards: Vec::new(),
            hooks: Vec::new(),
            hints: Vec::new(),
            tracker: Arc::new(NoopTracker),
            output_transformer: Arc::new(NoopOutputTransformer),
        }
    }

    /// Production default: defensive guards (loop detection,
    /// tool budget, catastrophic shell commands, workspace
    /// boundary, prompt injection), budget/iteration/truncation
    /// hints, adaptive concurrency (≤ 8), and the debug logging
    /// hook.
    ///
    /// Tuned to never interfere with legitimate work: the loop
    /// detector only fires on the 3rd *identical* consecutive
    /// call, the budget (250) exceeds the theoretical maximum of
    /// a bounded 25-iteration run, and the shell list only
    /// vetoes machine-level unrecoverable commands.
    pub fn default_policy(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            guards: vec![
                Arc::new(LoopDetectionGuard::new(3)),
                Arc::new(ToolBudgetGuard::new(250)),
                Arc::new(ShellDenyGuard::new()),
                Arc::new(WorkspaceBoundaryGuard::new(workspace_root)),
                Arc::new(PromptInjectionGuard::new()),
            ],
            hooks: vec![Arc::new(LoggingHook)],
            hints: vec![
                Arc::new(ContextBudgetHint::new(0.85)),
                Arc::new(IterationReminderHint::new(10)),
                Arc::new(TruncationHint),
            ],
            tracker: Arc::new(AdaptiveTracker::new(8)),
            output_transformer: Arc::new(NoopOutputTransformer),
        }
    }

    /// Add a guard (registration order = evaluation order).
    pub fn add_guard(mut self, guard: Arc<dyn Guard>) -> Self {
        self.guards.push(guard);
        self
    }

    /// Add a hook.
    pub fn add_hook(mut self, hook: Arc<dyn AgentHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    /// Add a hint.
    pub fn add_hint(mut self, hint: Arc<dyn Hint>) -> Self {
        self.hints.push(hint);
        self
    }

    /// Replace the tracker.
    pub fn with_tracker(mut self, tracker: Arc<dyn Tracker>) -> Self {
        self.tracker = tracker;
        self
    }

    /// Replace the output transformer.
    pub fn with_output_transformer(
        mut self,
        transformer: Arc<dyn OutputTransformer>,
    ) -> Self {
        self.output_transformer = transformer;
        self
    }

    /// True when the bundle cannot change observable behaviour
    /// (no guards / hooks / hints). Used by the loop to skip
    /// dead work on the hot path.
    pub fn is_inert(&self) -> bool {
        self.guards.is_empty() && self.hooks.is_empty() && self.hints.is_empty()
    }
}

impl Default for Steering {
    fn default() -> Self {
        Self::noop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The noop bundle MUST be inert.
    #[test]
    fn noop_bundle_is_inert() {
        assert!(Steering::noop().is_inert());
        assert!(Steering::default().is_inert());
    }

    /// The default policy MUST carry the full defensive set and
    /// therefore not be inert.
    #[test]
    fn default_policy_is_defensive() {
        let s = Steering::default_policy("/tmp/ws");
        assert!(!s.is_inert());
        assert_eq!(s.guards.len(), 5);
        assert_eq!(s.hints.len(), 3);
        assert_eq!(s.hooks.len(), 1);
        let guard_names: Vec<&str> =
            s.guards.iter().map(|g| g.name()).collect();
        assert_eq!(
            guard_names,
            vec![
                "loop_detection",
                "tool_budget",
                "shell_deny",
                "workspace_boundary",
                "prompt_injection",
            ]
        );
    }
}
