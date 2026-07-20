//! Unified `Hook` trait (PR-4.2).
//!
//! Defines the new `Hook` trait that replaces the dual
//! `synthia-agent::AgentHook` + `synthia-plugin::HookRunner` system.
//! The existing `AgentHook` trait is marked with `#[deprecated]`
//! and will be removed after the 3-month deprecation window.

use async_trait::async_trait;

use crate::outcome::{HookEvent, HookOutcome};

/// Unified hook trait operating on 10 typed events.
///
/// Every `on_event` call returns a [`HookOutcome`], which the hook
/// runner evaluates to decide whether to proceed, deny, or forward
/// the event to the main agent.
///
/// # Migration
///
/// The existing [`crate::traits::AgentHook`] trait is deprecated in
/// favor of this trait. An adapter (`AgentHookAdapter`) bridges the
/// old trait to the new one automatically.
#[async_trait]
pub trait Hook: Send + Sync + std::fmt::Debug {
    /// Handle a hook event.
    ///
    /// Returns [`HookOutcome::Allow`] by default (no-op hook).
    /// Override to inspect or modify the event flow.
    async fn on_event(&self, _event: &HookEvent) -> HookOutcome {
        HookOutcome::Allow
    }

    /// Human-readable name for diagnostics and logging.
    fn name(&self) -> &str {
        "unnamed-hook"
    }
}

/// Adapter that bridges the deprecated [`crate::traits::AgentHook`]
/// to the new [`Hook`] trait.
///
/// The adapter maps the old 6-method interface to the new 10-event
/// interface. Events that have no old equivalent (e.g., `PreCompact`,
/// `PostCompact`, `PreMessageDrop`) default to `Allow`.
#[derive(Debug)]
pub struct AgentHookAdapter<T: ?Sized> {
    inner: std::sync::Arc<T>,
}

impl<T: ?Sized> AgentHookAdapter<T> {
    /// Create a new adapter wrapping an `AgentHook` implementation.
    pub fn new(inner: std::sync::Arc<T>) -> Self {
        Self { inner }
    }
}

#[async_trait]
#[allow(deprecated)]
impl<T: crate::traits::AgentHook + ?Sized + 'static> Hook
    for AgentHookAdapter<T>
{
    async fn on_event(&self, event: &HookEvent) -> HookOutcome {
        use crate::outcome::*;
        match event {
            HookEvent::UserPromptSubmit(payload) => {
                let mut ctx = crate::traits::AgentContext::new(
                    payload.session_id.clone(),
                    String::new(),
                );
                let _ = self.inner.on_before_llm(&mut ctx).await;
                HookOutcome::Allow
            }
            HookEvent::PostResponse(payload) => {
                let ctx = crate::traits::AgentContext::new(
                    payload.session_id.clone(),
                    String::new(),
                );
                let response_json = serde_json::json!({
                    "content": payload.response_summary,
                });
                let _ = self.inner.on_after_llm(&ctx, &response_json).await;
                HookOutcome::Allow
            }
            HookEvent::PreToolUse(payload) => {
                let ctx = crate::traits::AgentContext::new(
                    payload.session_id.clone(),
                    String::new(),
                );
                let call_value = serde_json::json!({
                    "tool_name": payload.tool_name,
                    "input": payload.input,
                });
                match self.inner.on_before_tool(&ctx, &call_value).await {
                    Ok(crate::traits::ToolAction::Proceed) => {
                        HookOutcome::Allow
                    }
                    Ok(crate::traits::ToolAction::Skip) => HookOutcome::Deny {
                        reason: format!(
                            "tool {} skipped by hook",
                            payload.tool_name
                        ),
                    },
                    Ok(crate::traits::ToolAction::Modify(_)) => {
                        HookOutcome::Allow
                    }
                    Ok(crate::traits::ToolAction::PendingConfirm {
                        ..
                    }) => HookOutcome::Allow,
                    Err(e) => HookOutcome::Deny {
                        reason: e.to_string(),
                    },
                }
            }
            HookEvent::PostToolUse(payload) => {
                let ctx = crate::traits::AgentContext::new(
                    payload.session_id.clone(),
                    String::new(),
                );
                let call_value = serde_json::json!({
                    "tool_name": payload.tool_name,
                    "input": payload.input,
                });
                let _ = self
                    .inner
                    .on_after_tool(&ctx, &call_value, &payload.output)
                    .await;
                HookOutcome::Allow
            }
            // Events without a direct AgentHook equivalent default to Allow.
            HookEvent::SessionStart(_)
            | HookEvent::SessionEnd(_)
            | HookEvent::PreResponse(_)
            | HookEvent::PreCompact(_)
            | HookEvent::PostCompact(_)
            | HookEvent::PreMessageDrop(_) => HookOutcome::Allow,
        }
    }

    fn name(&self) -> &str {
        "agent-hook-adapter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct NoopHook;

    #[async_trait]
    impl Hook for NoopHook {
        async fn on_event(&self, _event: &HookEvent) -> HookOutcome {
            HookOutcome::Allow
        }

        fn name(&self) -> &str {
            "noop"
        }
    }

    #[tokio::test]
    async fn default_on_event_returns_allow() {
        #[derive(Debug)]
        struct DefaultHook;

        #[async_trait]
        impl Hook for DefaultHook {}

        let hook = DefaultHook;
        let event =
            HookEvent::SessionStart(crate::outcome::SessionStartPayload {
                session_id: "s".into(),
            });
        assert_eq!(hook.on_event(&event).await, HookOutcome::Allow);
    }

    #[tokio::test]
    async fn custom_hook_overrides_on_event() {
        let hook = NoopHook;
        let event =
            HookEvent::SessionStart(crate::outcome::SessionStartPayload {
                session_id: "s".into(),
            });
        assert_eq!(hook.on_event(&event).await, HookOutcome::Allow);
        assert_eq!(hook.name(), "noop");
    }

    #[tokio::test]
    async fn trait_object_safe() {
        let hook: std::sync::Arc<dyn Hook> = std::sync::Arc::new(NoopHook);
        let event =
            HookEvent::SessionStart(crate::outcome::SessionStartPayload {
                session_id: "s".into(),
            });
        assert_eq!(hook.on_event(&event).await, HookOutcome::Allow);
    }
}
