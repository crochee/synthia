//! Hooks system for synthia-agent
//!
//! This module provides a lifecycle extension mechanism through hooks.
//! Hooks allow injecting custom logic at various points in the agent lifecycle.
//!
//! ## Architecture
//!
//! - [`Hook`] - Trait for implementing custom hooks
//! - [`HookRegistry`] - Registry for managing hooks with concurrent execution
//! - [`HookEvent`] - Events that trigger hook execution
//!
//! ## Example
//!
//! ```rust,ignore
//! use synthia_agent::hooks::{Hook, HookEvent, HookRegistry};
//! use async_trait::async_trait;
//!
//! struct MyHook;
//!
//! #[async_trait]
//! impl Hook for MyHook {
//!     fn name(&self) -> &str {
//!         "my_hook"
//!     }
//!
//!     async fn on_event(&self, event: &HookEvent) -> Result<()> {
//!         println!("Event: {:?}", event);
//!         Ok(())
//!     }
//! }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use futures::{StreamExt, stream::FuturesUnordered};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::Result;

pub mod events;
pub mod phases;

pub use events::*;
pub use phases::{HookPhase, PhaseOrder};

/// Trait for implementing custom hooks.
///
/// Hooks are called at various points during the agent lifecycle
/// to allow for custom behavior injection.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Returns the name of the hook for identification.
    fn name(&self) -> &str;

    /// Called when an event occurs.
    ///
    /// # Arguments
    ///
    /// * `event` - The event that triggered this hook call
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if the hook fails.
    async fn on_event(&self, event: &HookEvent) -> Result<()>;
}

/// Events that can trigger hook execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    /// Emitted before the agent starts processing
    BeforeAgentStart { session_id: String },
    /// Emitted after the agent finishes processing
    AfterAgentEnd { session_id: String, success: bool },
    /// Emitted before making an LLM call
    BeforeLLMCall { model: String, message_count: usize },
    /// Emitted after an LLM call completes
    AfterLLMCall {
        model: String,
        tokens_used: Option<u64>,
        success: bool,
    },
    /// Emitted before a ReAct step begins
    BeforeStep { session_id: String, step: u32 },
    /// Emitted after a ReAct step completes
    AfterStep {
        session_id: String,
        step: u32,
        tool_count: usize,
    },
    /// Emitted before a turn completes (all tools finished)
    BeforeTurnComplete { session_id: String, turn_id: String },
    /// Emitted after a turn completes
    AfterTurnComplete {
        session_id: String,
        turn_id: String,
        has_errors: bool,
    },
    /// Emitted before executing a tool
    BeforeToolCall { tool: String, args: Value },
    /// Emitted after a tool execution completes
    AfterToolCall {
        tool: String,
        args: Value,
        success: bool,
    },
    /// Emitted when a session starts
    SessionStart { session_id: String },
    /// Emitted when a session ends
    SessionEnd {
        session_id: String,
        message_count: usize,
    },
    /// Emitted when context compaction occurs
    ContextCompaction {
        messages_removed: usize,
        tokens_saved: u64,
    },
    /// Emitted after an agent completes
    AfterAgent { agent_id: String, result: String },
    /// Emitted after a tool use completes
    AfterToolUse {
        tool: String,
        args: Value,
        result: String,
    },
    /// Emitted when tool scheduling plan is created
    ToolSchedulingPlan(events::ToolSchedulingPlan),
    /// Emitted after a batch of tools completes (within a phase)
    AfterToolBatchComplete(events::AfterToolBatchComplete),
}

/// Type alias for a pointer to a Hook trait object.
pub type HookPtr = Arc<dyn Hook>;

/// Registry for managing hooks with concurrent execution support.
///
/// The registry allows registering, unregistering, and emitting events
/// to all registered hooks. Events are emitted concurrently for better
/// performance when multiple hooks are registered.
#[derive(Default)]
pub struct HookRegistry {
    hooks: RwLock<Vec<HookPtr>>,
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookRegistry")
            .field(
                "hook_count",
                &self.hooks.try_read().map(|g| g.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl HookRegistry {
    /// Creates a new empty hook registry.
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(Vec::new()),
        }
    }

    /// Registers a new hook.
    ///
    /// # Arguments
    ///
    /// * `hook` - The hook to register
    pub async fn register(&self, hook: HookPtr) {
        let mut hooks = self.hooks.write().await;
        hooks.push(hook);
    }

    /// Unregisters a hook by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the hook to unregister
    ///
    /// # Returns
    ///
    /// `true` if a hook was removed, `false` otherwise.
    pub async fn unregister(&self, name: &str) -> bool {
        let mut hooks = self.hooks.write().await;
        let initial_len = hooks.len();
        hooks.retain(|h| h.name() != name);
        hooks.len() != initial_len
    }

    /// Emits an event to all registered hooks concurrently.
    ///
    /// All hooks are executed in parallel, and errors are logged
    /// but do not prevent other hooks from executing.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to emit
    pub async fn emit(&self, event: &HookEvent) {
        let hooks = self.hooks.read().await;

        if hooks.is_empty() {
            return;
        }

        let futures: FuturesUnordered<_> = hooks
            .iter()
            .map(|hook| {
                let hook = Arc::clone(hook);
                let event = event.clone();
                async move {
                    if let Err(e) = hook.on_event(&event).await {
                        tracing::warn!(
                            hook_name = hook.name(),
                            error = %e,
                            "Hook execution failed"
                        );
                    }
                }
            })
            .collect();

        futures.collect::<()>().await;
    }

    /// Returns the number of registered hooks.
    pub async fn hook_count(&self) -> usize {
        self.hooks.read().await.len()
    }

    /// Emits an event to all registered hooks in phase-aware order.
    ///
    /// Hooks are executed concurrently, but this method ensures that events
    /// are processed in the correct phase order as defined by HookPhase.
    /// Errors are logged but do not stop other hooks from executing.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to emit
    pub async fn emit_ordered(&self, event: &HookEvent) {
        let hooks = self.hooks.read().await;

        if hooks.is_empty() {
            return;
        }

        // For now, emit all hooks concurrently maintaining backward compatibility.
        // The phase ordering is a hint for external coordination.
        let futures: FuturesUnordered<_> = hooks
            .iter()
            .map(|hook| {
                let hook = Arc::clone(hook);
                let event = event.clone();
                async move {
                    if let Err(e) = hook.on_event(&event).await {
                        tracing::warn!(
                            hook_name = hook.name(),
                            error = %e,
                            "Hook execution failed"
                        );
                    }
                }
            })
            .collect();

        futures.collect::<()>().await;
    }
}
#[derive(Debug, Clone, Copy)]
pub struct LoggingHook;

impl LoggingHook {
    /// Creates a new logging hook.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LoggingHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for LoggingHook {
    fn name(&self) -> &str {
        "logging"
    }

    async fn on_event(&self, event: &HookEvent) -> Result<()> {
        match event {
            HookEvent::BeforeAgentStart { session_id } => {
                tracing::info!(session_id = %session_id, "Agent starting");
            }
            HookEvent::AfterAgentEnd {
                session_id,
                success,
            } => {
                tracing::info!(session_id = %session_id, success = success, "Agent ended");
            }
            HookEvent::BeforeLLMCall {
                model,
                message_count,
            } => {
                tracing::debug!(model = %model, message_count = message_count, "LLM call starting");
            }
            HookEvent::AfterLLMCall {
                model,
                tokens_used,
                success,
            } => {
                tracing::debug!(model = %model, tokens_used = ?tokens_used, success = success, "LLM call completed");
            }
            HookEvent::BeforeStep { session_id, step } => {
                tracing::debug!(session_id = %session_id, step = step, "Step starting");
            }
            HookEvent::AfterStep {
                session_id,
                step,
                tool_count,
            } => {
                tracing::debug!(session_id = %session_id, step = step, tool_count = tool_count, "Step completed");
            }
            HookEvent::BeforeTurnComplete {
                session_id,
                turn_id,
            } => {
                tracing::debug!(session_id = %session_id, turn_id = %turn_id, "Turn completing");
            }
            HookEvent::AfterTurnComplete {
                session_id,
                turn_id,
                has_errors,
            } => {
                tracing::debug!(session_id = %session_id, turn_id = %turn_id, has_errors = has_errors, "Turn completed");
            }
            HookEvent::BeforeToolCall { tool, args } => {
                tracing::debug!(tool = %tool, args = ?args, "Tool call starting");
            }
            HookEvent::AfterToolCall {
                tool,
                args,
                success,
            } => {
                tracing::debug!(tool = %tool, args = ?args, success = success, "Tool call completed");
            }
            HookEvent::SessionStart { session_id } => {
                tracing::info!(session_id = %session_id, "Session started");
            }
            HookEvent::SessionEnd {
                session_id,
                message_count,
            } => {
                tracing::info!(session_id = %session_id, message_count = message_count, "Session ended");
            }
            HookEvent::ContextCompaction {
                messages_removed,
                tokens_saved,
            } => {
                tracing::info!(
                    messages_removed = messages_removed,
                    tokens_saved = tokens_saved,
                    "Context compacted"
                );
            }
            HookEvent::AfterAgent { agent_id, result } => {
                tracing::info!(agent_id = %agent_id, result = %result, "Agent completed");
            }
            HookEvent::AfterToolUse { tool, args, result } => {
                tracing::info!(tool = %tool, args = ?args, result = %result, "Tool use completed");
            }
            HookEvent::ToolSchedulingPlan(plan) => {
                tracing::debug!(
                    session_id = %plan.session_id,
                    turn_id = %plan.turn_id,
                    tool_count = plan.tools.len(),
                    total_phases = plan.schedule.phases.len(),
                    "Tool scheduling plan created"
                );
            }
            HookEvent::AfterToolBatchComplete(batch) => {
                tracing::debug!(
                    session_id = %batch.session_id,
                    batch_id = batch.batch_id,
                    tool_count = batch.tool_count,
                    has_errors = batch.has_errors,
                    "Tool batch complete"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    struct TestHook {
        name: String,
        call_count: Arc<AtomicUsize>,
    }

    impl TestHook {
        fn new(name: &str, call_count: Arc<AtomicUsize>) -> Self {
            Self {
                name: name.to_string(),
                call_count,
            }
        }
    }

    #[async_trait]
    impl Hook for TestHook {
        fn name(&self) -> &str {
            &self.name
        }

        async fn on_event(&self, _event: &HookEvent) -> Result<()> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// A hook that always fails for testing error handling
    struct FailingHook;

    impl FailingHook {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl Hook for FailingHook {
        fn name(&self) -> &str {
            "failing"
        }

        async fn on_event(&self, _event: &HookEvent) -> Result<()> {
            Err("Hook failed".into())
        }
    }

    /// A hook that captures the event for verification
    struct EventCapturingHook {
        name: String,
        captured_event: std::sync::Mutex<Option<HookEvent>>,
    }

    impl EventCapturingHook {
        fn new(name: &str) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                captured_event: std::sync::Mutex::new(None),
            })
        }

        fn get_captured(&self) -> Option<HookEvent> {
            self.captured_event.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Hook for EventCapturingHook {
        fn name(&self) -> &str {
            &self.name
        }

        async fn on_event(&self, event: &HookEvent) -> Result<()> {
            *self.captured_event.lock().unwrap() = Some(event.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_hook_registry_register() {
        let registry = HookRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let hook = Arc::new(TestHook::new("test", Arc::clone(&call_count)));

        registry.register(hook).await;
        assert_eq!(registry.hook_count().await, 1);
    }

    #[tokio::test]
    async fn test_hook_registry_unregister() {
        let registry = HookRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let hook = Arc::new(TestHook::new("test", Arc::clone(&call_count)));

        registry.register(hook).await;
        assert_eq!(registry.hook_count().await, 1);

        let removed = registry.unregister("test").await;
        assert!(removed);
        assert_eq!(registry.hook_count().await, 0);
    }

    #[tokio::test]
    async fn test_hook_registry_unregister_nonexistent() {
        let registry = HookRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let hook = Arc::new(TestHook::new("test", Arc::clone(&call_count)));

        registry.register(hook).await;

        // Try to unregister a hook that doesn't exist
        let removed = registry.unregister("nonexistent").await;
        assert!(!removed);

        // Original hook should still be there
        assert_eq!(registry.hook_count().await, 1);
    }

    #[tokio::test]
    async fn test_hook_registry_emit() {
        let registry = HookRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let hook = Arc::new(TestHook::new("test", Arc::clone(&call_count)));

        registry.register(hook).await;

        let event = HookEvent::SessionStart {
            session_id: "test-session".to_string(),
        };
        registry.emit(&event).await;

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_multiple_hooks_concurrent() {
        let registry = HookRegistry::new();
        let call_count1 = Arc::new(AtomicUsize::new(0));
        let call_count2 = Arc::new(AtomicUsize::new(0));

        registry
            .register(Arc::new(TestHook::new(
                "hook1",
                Arc::clone(&call_count1),
            )))
            .await;
        registry
            .register(Arc::new(TestHook::new(
                "hook2",
                Arc::clone(&call_count2),
            )))
            .await;

        let event = HookEvent::BeforeAgentStart {
            session_id: "test".to_string(),
        };
        registry.emit(&event).await;

        assert_eq!(call_count1.load(Ordering::SeqCst), 1);
        assert_eq!(call_count2.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_hook_error_does_not_stop_other_hooks() {
        let registry = HookRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let failing_hook = Arc::new(FailingHook::new());
        let good_hook =
            Arc::new(TestHook::new("good", Arc::clone(&call_count)));

        registry.register(failing_hook).await;
        registry.register(good_hook).await;

        let event = HookEvent::SessionStart {
            session_id: "test".to_string(),
        };
        registry.emit(&event).await;

        // Good hook should still have been called despite failing hook
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_logging_hook() {
        let hook = LoggingHook::new();
        assert_eq!(hook.name(), "logging");

        let event = HookEvent::SessionStart {
            session_id: "test".to_string(),
        };
        let result = hook.on_event(&event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logging_hook_all_event_variants() {
        let hook = LoggingHook::new();

        // Test all HookEvent variants
        let events = vec![
            HookEvent::BeforeAgentStart {
                session_id: "s1".to_string(),
            },
            HookEvent::AfterAgentEnd {
                session_id: "s1".to_string(),
                success: true,
            },
            HookEvent::BeforeLLMCall {
                model: "gpt-4".to_string(),
                message_count: 5,
            },
            HookEvent::AfterLLMCall {
                model: "gpt-4".to_string(),
                tokens_used: Some(100),
                success: true,
            },
            HookEvent::BeforeStep {
                session_id: "s1".to_string(),
                step: 1,
            },
            HookEvent::AfterStep {
                session_id: "s1".to_string(),
                step: 1,
                tool_count: 3,
            },
            HookEvent::BeforeTurnComplete {
                session_id: "s1".to_string(),
                turn_id: "t1".to_string(),
            },
            HookEvent::AfterTurnComplete {
                session_id: "s1".to_string(),
                turn_id: "t1".to_string(),
                has_errors: false,
            },
            HookEvent::BeforeToolCall {
                tool: "bash".to_string(),
                args: serde_json::json!({"cmd": "ls"}),
            },
            HookEvent::AfterToolCall {
                tool: "bash".to_string(),
                args: serde_json::json!({"cmd": "ls"}),
                success: true,
            },
            HookEvent::SessionStart {
                session_id: "s1".to_string(),
            },
            HookEvent::SessionEnd {
                session_id: "s1".to_string(),
                message_count: 10,
            },
            HookEvent::ContextCompaction {
                messages_removed: 5,
                tokens_saved: 1000,
            },
            HookEvent::AfterAgent {
                agent_id: "a1".to_string(),
                result: "success".to_string(),
            },
            HookEvent::AfterToolUse {
                tool: "read".to_string(),
                args: serde_json::json!({"path": "/tmp"}),
                result: "file content".to_string(),
            },
        ];

        for event in events {
            let result = hook.on_event(&event).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_emit_empty_registry() {
        let registry = HookRegistry::new();

        let event = HookEvent::SessionStart {
            session_id: "test".to_string(),
        };

        registry.emit(&event).await;
    }

    #[tokio::test]
    async fn test_concurrent_hook_execution() {
        let registry = HookRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for i in 0..10 {
            registry
                .register(Arc::new(TestHook::new(
                    &format!("hook-{i}"),
                    Arc::clone(&call_count),
                )))
                .await;
        }

        let event = HookEvent::SessionStart {
            session_id: "test".to_string(),
        };
        registry.emit(&event).await;

        assert_eq!(call_count.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn test_hooks_receive_correct_event_data() {
        let registry = HookRegistry::new();
        let capturing_hook = EventCapturingHook::new("capturer");

        registry
            .register(Arc::clone(&capturing_hook) as HookPtr)
            .await;

        let event = HookEvent::BeforeLLMCall {
            model: "claude-3".to_string(),
            message_count: 42,
        };
        registry.emit(&event).await;

        let captured = capturing_hook.get_captured();
        assert!(captured.is_some());

        match captured.unwrap() {
            HookEvent::BeforeLLMCall {
                model,
                message_count,
            } => {
                assert_eq!(model, "claude-3");
                assert_eq!(message_count, 42);
            }
            other => panic!("Expected BeforeLLMCall, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_hook_event_serialization_all_variants() {
        use serde_json;

        let events = vec![
            HookEvent::BeforeAgentStart {
                session_id: "test-session".to_string(),
            },
            HookEvent::AfterAgentEnd {
                session_id: "test-session".to_string(),
                success: false,
            },
            HookEvent::BeforeLLMCall {
                model: "gpt-4o".to_string(),
                message_count: 10,
            },
            HookEvent::AfterLLMCall {
                model: "gpt-4o".to_string(),
                tokens_used: None,
                success: true,
            },
            HookEvent::BeforeStep {
                session_id: "step-session".to_string(),
                step: 5,
            },
            HookEvent::AfterStep {
                session_id: "step-session".to_string(),
                step: 5,
                tool_count: 3,
            },
            HookEvent::BeforeTurnComplete {
                session_id: "turn-session".to_string(),
                turn_id: "turn-1".to_string(),
            },
            HookEvent::AfterTurnComplete {
                session_id: "turn-session".to_string(),
                turn_id: "turn-1".to_string(),
                has_errors: true,
            },
            HookEvent::BeforeToolCall {
                tool: "filesystem_read".to_string(),
                args: serde_json::json!({"path": "/tmp/test.txt"}),
            },
            HookEvent::AfterToolCall {
                tool: "filesystem_read".to_string(),
                args: serde_json::json!({"path": "/tmp/test.txt"}),
                success: true,
            },
            HookEvent::SessionStart {
                session_id: "session-123".to_string(),
            },
            HookEvent::SessionEnd {
                session_id: "session-123".to_string(),
                message_count: 50,
            },
            HookEvent::ContextCompaction {
                messages_removed: 20,
                tokens_saved: 5000,
            },
            HookEvent::AfterAgent {
                agent_id: "agent-1".to_string(),
                result: "completed".to_string(),
            },
            HookEvent::AfterToolUse {
                tool: "web_search".to_string(),
                args: serde_json::json!({"query": "rust programming"}),
                result: "Found 100 results".to_string(),
            },
        ];

        for event in events {
            // Serialize
            let json = serde_json::to_string(&event).expect("Should serialize");
            // Deserialize
            let deserialized: HookEvent =
                serde_json::from_str(&json).expect("Should deserialize");

            // Verify by re-serializing and comparing JSON strings
            let deser_json =
                serde_json::to_string(&deserialized).expect("Should serialize");
            assert_eq!(json, deser_json);
        }
    }

    #[tokio::test]
    async fn test_debug_trait_hook_registry() {
        let registry = HookRegistry::new();
        let debug_str = format!("{registry:?}");
        assert!(debug_str.contains("HookRegistry"));
        assert!(debug_str.contains("hook_count"));
    }

    #[tokio::test]
    async fn test_debug_trait_logging_hook() {
        let hook = LoggingHook::new();
        let debug_str = format!("{hook:?}");
        assert!(debug_str.contains("LoggingHook"));
    }

    #[tokio::test]
    async fn test_debug_trait_hook_event() {
        let event = HookEvent::BeforeAgentStart {
            session_id: "dbg".to_string(),
        };
        let debug_str = format!("{event:?}");
        assert!(debug_str.contains("BeforeAgentStart"));
        assert!(debug_str.contains("dbg"));
    }

    #[tokio::test]
    async fn test_hook_registry_new_is_empty() {
        let registry = HookRegistry::new();
        assert_eq!(registry.hook_count().await, 0);
    }

    #[tokio::test]
    async fn test_unregister_allows_reregister() {
        let registry = HookRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let hook = Arc::new(TestHook::new("test", Arc::clone(&call_count)));
        registry.register(Arc::clone(&hook) as HookPtr).await;
        assert_eq!(registry.hook_count().await, 1);

        registry.unregister("test").await;
        assert_eq!(registry.hook_count().await, 0);

        // Register with same name after unregister
        let hook2 = Arc::new(TestHook::new("test", Arc::clone(&call_count)));
        registry.register(hook2).await;
        assert_eq!(registry.hook_count().await, 1);
    }

    #[tokio::test]
    async fn test_multiple_hooks_emit_waits_for_all() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let registry = HookRegistry::new();
        let barrier = Arc::new(AtomicBool::new(false));
        let call_count = Arc::new(AtomicUsize::new(0));

        let barrier_hook = {
            let _barrier = Arc::clone(&barrier);
            let call_count = Arc::clone(&call_count);
            Arc::new(TestHook::new("barrier", Arc::clone(&call_count)))
        };

        registry.register(barrier_hook).await;

        let event = HookEvent::SessionStart {
            session_id: "sync-test".to_string(),
        };
        registry.emit(&event).await;

        // After emit completes, all hooks should have been called
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_logging_hook_default() {
        let hook = LoggingHook;
        assert_eq!(hook.name(), "logging");
    }
}
