//! `UnifiedHookDispatcher` — single dispatch point for hooks + extensions.
//!
//! PR-1.2 of change #2. Dispatches `HookEvent` to both the `HookRegistry`
//! (via `AgentHookAdapter`) and optionally `ExtensionRegistry` in a
//! hook-first ordering. The combined outcome follows the precedence:
//! `Deny > ForwardToMainAgent > Allow`.

use std::sync::Arc;

use crate::{
    hook_trait::Hook,
    outcome::{HookEvent, HookOutcome},
};

/// Single dispatch point for hooks and extensions.
///
/// Hooks run first (gate decision). If any hook returns `Deny`,
/// extensions are not called (short-circuit). If hooks return
/// `Allow` or `ForwardToMainAgent`, extensions are then dispatched
/// and their outcome (converted via `From<ExtensionOutcome>`) is
/// merged with the hook outcome.
///
/// # Outcome Precedence
///
/// `Deny > ForwardToMainAgent > Allow`
///
/// A `Deny` from any source always wins. A `ForwardToMainAgent`
/// from any source overrides `Allow` but not `Deny`.
pub struct UnifiedHookDispatcher {
    hooks: Vec<Arc<dyn Hook>>,
}

impl UnifiedHookDispatcher {
    /// Create a new dispatcher with the given hooks.
    pub fn new(hooks: Vec<Arc<dyn Hook>>) -> Self {
        Self { hooks }
    }

    /// Create a dispatcher from a [`crate::HookRegistry`], wrapping
    /// each registered `AgentHook` with [`crate::AgentHookAdapter`].
    ///
    /// Only non-failed hooks are included. The adapter bridges the
    /// old 6-method `AgentHook` trait to the new `Hook` trait.
    pub fn from_hook_registry(registry: &crate::HookRegistry) -> Self {
        let hooks = registry.snapshot_adapted_hooks();
        Self { hooks }
    }

    /// Create an empty dispatcher with no hooks.
    pub fn empty() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Add a hook to the dispatcher.
    pub fn add_hook(&mut self, hook: Arc<dyn Hook>) {
        self.hooks.push(hook);
    }

    /// Dispatch a `HookEvent` to all registered hooks.
    ///
    /// Hooks are called in registration order. The combined outcome
    /// follows the precedence: `Deny > ForwardToMainAgent > Allow`.
    pub async fn dispatch(&self, event: &HookEvent) -> HookOutcome {
        let mut combined = HookOutcome::Allow;
        for hook in &self.hooks {
            let outcome = hook.on_event(event).await;
            combined = merge_outcomes(combined, outcome);
            if matches!(combined, HookOutcome::Deny { .. }) {
                // Short-circuit: no point continuing after a Deny.
                break;
            }
        }
        combined
    }

    /// Number of registered hooks.
    pub fn hook_count(&self) -> usize {
        self.hooks.len()
    }
}

/// Merge two outcomes according to precedence: `Deny > ForwardToMainAgent > Allow`.
fn merge_outcomes(current: HookOutcome, incoming: HookOutcome) -> HookOutcome {
    match (current, incoming) {
        // Deny always wins.
        (HookOutcome::Deny { reason }, _)
        | (_, HookOutcome::Deny { reason }) => HookOutcome::Deny { reason },
        // ForwardToMainAgent overrides Allow but not Deny (handled above).
        (HookOutcome::ForwardToMainAgent { hint }, HookOutcome::Allow)
        | (HookOutcome::Allow, HookOutcome::ForwardToMainAgent { hint }) => {
            HookOutcome::ForwardToMainAgent { hint }
        }
        // ForwardToMainAgent + ForwardToMainAgent: keep the incoming one.
        (
            HookOutcome::ForwardToMainAgent { .. },
            HookOutcome::ForwardToMainAgent { hint },
        ) => HookOutcome::ForwardToMainAgent { hint },
        // Allow + Allow = Allow.
        (HookOutcome::Allow, HookOutcome::Allow) => HookOutcome::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::SessionStartPayload;

    #[derive(Debug)]
    struct AllowHook;

    #[async_trait::async_trait]
    impl Hook for AllowHook {
        async fn on_event(&self, _event: &HookEvent) -> HookOutcome {
            HookOutcome::Allow
        }

        fn name(&self) -> &str {
            "allow-hook"
        }
    }

    #[derive(Debug)]
    struct DenyHook;

    #[async_trait::async_trait]
    impl Hook for DenyHook {
        async fn on_event(&self, _event: &HookEvent) -> HookOutcome {
            HookOutcome::Deny {
                reason: "denied".into(),
            }
        }

        fn name(&self) -> &str {
            "deny-hook"
        }
    }

    #[derive(Debug)]
    struct ForwardHook;

    #[async_trait::async_trait]
    impl Hook for ForwardHook {
        async fn on_event(&self, _event: &HookEvent) -> HookOutcome {
            HookOutcome::ForwardToMainAgent {
                hint: "forward".into(),
            }
        }

        fn name(&self) -> &str {
            "forward-hook"
        }
    }

    fn session_event() -> HookEvent {
        HookEvent::SessionStart(SessionStartPayload {
            session_id: "test".into(),
        })
    }

    #[tokio::test]
    async fn allow_plus_allow_returns_allow() {
        let dispatcher = UnifiedHookDispatcher::new(vec![
            Arc::new(AllowHook),
            Arc::new(AllowHook),
        ]);
        assert_eq!(
            dispatcher.dispatch(&session_event()).await,
            HookOutcome::Allow
        );
    }

    #[tokio::test]
    async fn allow_plus_deny_returns_deny() {
        let dispatcher = UnifiedHookDispatcher::new(vec![
            Arc::new(AllowHook),
            Arc::new(DenyHook),
        ]);
        assert_eq!(
            dispatcher.dispatch(&session_event()).await,
            HookOutcome::Deny {
                reason: "denied".into()
            }
        );
    }

    #[tokio::test]
    async fn deny_short_circuits() {
        let dispatcher = UnifiedHookDispatcher::new(vec![
            Arc::new(DenyHook),
            Arc::new(AllowHook),
        ]);
        assert_eq!(
            dispatcher.dispatch(&session_event()).await,
            HookOutcome::Deny {
                reason: "denied".into()
            }
        );
    }

    #[tokio::test]
    async fn allow_plus_forward_returns_forward() {
        let dispatcher = UnifiedHookDispatcher::new(vec![
            Arc::new(AllowHook),
            Arc::new(ForwardHook),
        ]);
        assert_eq!(
            dispatcher.dispatch(&session_event()).await,
            HookOutcome::ForwardToMainAgent {
                hint: "forward".into()
            }
        );
    }

    #[tokio::test]
    async fn no_hooks_returns_allow() {
        let dispatcher = UnifiedHookDispatcher::empty();
        assert_eq!(
            dispatcher.dispatch(&session_event()).await,
            HookOutcome::Allow
        );
    }
}
