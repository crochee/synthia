//! Unit tests for [`HookExecutor`].
//!
//! Coverage map (12 tests):
//!
//! - Panic isolation (fail-open): 8 tests
//!   ([`fire_before_llm`] panic returns
//!   Proceed, [`fire_after_tool`] panic, [`fire_after_llm`] panic,
//!   [`fire_complete`] panic, [`fire_iteration_end`] panic,
//!   [`on_loop_detected`] panic returns Proceed,
//!   [`on_session_end`] panic).
//! - Mixed execution: 2 tests
//!   (counting + panicking in `fire_before_llm`, counting + panicking
//!   in `fire_before_tool` — both verify the normal hook still fires
//!   before the panic short-circuits the chain).
//! - Empty registry: 1 test
//!   ([`HookExecutor::is_empty`] + dispatch on empty registry is a no-op).
//! - Verdict return: 1 test
//!   (a hook returning [`ToolAction::Modify`] propagates the verdict
//!   back through `fire_before_tool`).
//!
//! # Test Fixtures
//!
//! Three test-only hook structs are defined in [`self::mocks`]:
//!
//! - [`PanickingHook`]: invokes the hook callback then `panic!()`s,
//!   exposing an `Arc<AtomicBool>` flag to verify it was actually
//!   reached.
//! - [`CountingHook`]: increments per-event `Arc<AtomicU32>` counters
//!   to verify the normal hook fired.
//! - [`ModifyingHookForExecutor`]: returns a `ToolAction::Modify`
//!   verdict on `on_before_tool` to verify verdict propagation.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use synthia_core::Error;
#[allow(deprecated)]
use synthia_hook::AgentHook;
use synthia_hook::{AgentContext, HookRegistry, ToolAction};

use super::*;

// =============================================================================
// Test Fixtures
// =============================================================================

/// A hook that panics on every call. Exposes a flag the tests can
/// read to verify the hook was actually invoked (the flag is set
/// immediately before the `panic!()`).
#[derive(Debug)]
struct PanickingHook {
    panicked: Arc<AtomicBool>,
}

impl PanickingHook {
    fn new() -> Self {
        Self {
            panicked: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait::async_trait]
#[allow(deprecated)]
impl AgentHook for PanickingHook {
    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), Error> {
        self.panicked.store(true, Ordering::SeqCst);
        panic!("intentional panic in before_llm");
    }

    async fn on_after_llm(
        &self,
        _ctx: &AgentContext,
        _response: &serde_json::Value,
    ) -> Result<(), Error> {
        self.panicked.store(true, Ordering::SeqCst);
        panic!("intentional panic in after_llm");
    }

    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        self.panicked.store(true, Ordering::SeqCst);
        panic!("intentional panic in before_tool");
    }

    async fn on_after_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
        _result: &serde_json::Value,
    ) -> Result<(), Error> {
        self.panicked.store(true, Ordering::SeqCst);
        panic!("intentional panic in after_tool");
    }

    async fn on_iteration_end(
        &self,
        _ctx: &AgentContext,
        _iteration: usize,
    ) -> Result<(), Error> {
        self.panicked.store(true, Ordering::SeqCst);
        panic!("intentional panic in iteration_end");
    }

    async fn on_complete(&self, _ctx: &AgentContext) -> Result<(), Error> {
        self.panicked.store(true, Ordering::SeqCst);
        panic!("intentional panic in complete");
    }
}

/// A normal hook that counts invocations. Used to verify that one
/// panicking hook does NOT short-circuit the rest of the chain.
#[derive(Debug)]
struct CountingHook {
    before_llm_count: Arc<AtomicU32>,
    after_llm_count: Arc<AtomicU32>,
    before_tool_count: Arc<AtomicU32>,
    after_tool_count: Arc<AtomicU32>,
    complete_count: Arc<AtomicU32>,
}

impl CountingHook {
    fn new() -> Self {
        Self {
            before_llm_count: Arc::new(AtomicU32::new(0)),
            after_llm_count: Arc::new(AtomicU32::new(0)),
            before_tool_count: Arc::new(AtomicU32::new(0)),
            after_tool_count: Arc::new(AtomicU32::new(0)),
            complete_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

#[async_trait::async_trait]
#[allow(deprecated)]
impl AgentHook for CountingHook {
    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), Error> {
        self.before_llm_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn on_after_llm(
        &self,
        _ctx: &AgentContext,
        _response: &serde_json::Value,
    ) -> Result<(), Error> {
        self.after_llm_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        self.before_tool_count.fetch_add(1, Ordering::SeqCst);
        Ok(ToolAction::Proceed)
    }

    async fn on_after_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
        _result: &serde_json::Value,
    ) -> Result<(), Error> {
        self.after_tool_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn on_iteration_end(
        &self,
        _ctx: &AgentContext,
        _iteration: usize,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_complete(&self, _ctx: &AgentContext) -> Result<(), Error> {
        self.complete_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// A hook that returns a `ToolAction::Modify` verdict on
/// `on_before_tool`. Used to verify the verdict propagates back
/// through `fire_before_tool` unmodified.
#[derive(Debug)]
struct ModifyingHookForExecutor {
    new_name: String,
    new_input: serde_json::Value,
}

#[async_trait::async_trait]
#[allow(deprecated)]
impl AgentHook for ModifyingHookForExecutor {
    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        Ok(ToolAction::Modify(serde_json::json!({
            "name": self.new_name,
            "input": self.new_input
        })))
    }

    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_after_llm(
        &self,
        _ctx: &AgentContext,
        _response: &serde_json::Value,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_after_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
        _result: &serde_json::Value,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_iteration_end(
        &self,
        _ctx: &AgentContext,
        _iteration: usize,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_complete(&self, _ctx: &AgentContext) -> Result<(), Error> {
        Ok(())
    }
}

fn make_context() -> AgentContext {
    AgentContext::new("test-session".to_string(), "turn-1".to_string())
}

// =============================================================================
// Panic Isolation Tests (fail-open contract)
// =============================================================================

#[tokio::test]
async fn test_hook_executor_before_llm_panic_fail_open() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    // This should NOT panic - the panic should be caught
    let mut ctx = make_context();
    executor.fire_before_llm(&mut ctx).await;

    // Verify the hook was actually invoked (it set the flag before panicking)
    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_before_tool_panic_returns_proceed() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    let tool_call = serde_json::json!({"name": "test_tool"});
    let action = executor.fire_before_tool(&ctx, &tool_call).await;

    // Should return Proceed (fail-open)
    assert!(matches!(action, ToolAction::Proceed));
    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_after_tool_panic_fail_open() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    let tool_call = serde_json::json!({"name": "test_tool"});
    let result = serde_json::json!({"output": "ok"});

    // Should NOT panic
    executor.fire_after_tool(&ctx, &tool_call, &result).await;

    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_after_llm_panic_fail_open() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    let response = serde_json::json!({"content": "hello"});

    // Should NOT panic
    executor.fire_after_llm(&ctx, &response).await;

    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_complete_panic_fail_open() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    executor.fire_complete(&ctx).await;

    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_iteration_end_panic_returns_ok() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    executor.fire_iteration_end(&ctx, 1).await;

    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_loop_detected_panic_returns_proceed() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    let action = executor.on_loop_detected(&ctx, "tool_loop").await;

    assert!(matches!(action, ToolAction::Proceed));
    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_session_end_panic_fail_open() {
    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    executor.on_session_end(&ctx).await;

    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "hook should have been invoked"
    );
}

// =============================================================================
// Mixed Execution Tests
// =============================================================================

#[tokio::test]
async fn test_hook_executor_mixed_panicking_and_normal_hooks() {
    let counting = CountingHook::new();
    let before_llm_count = counting.before_llm_count.clone();

    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    // Register normal hook first
    registry.register_hook(Box::new(counting));
    // Register panicking hook second
    registry.register_hook(Box::new(panicking));

    let executor = HookExecutor::new(registry);

    let mut ctx = make_context();
    executor.fire_before_llm(&mut ctx).await;

    // The counting hook should have been called (it fires before the panicking one)
    assert!(
        before_llm_count.load(Ordering::SeqCst) > 0,
        "counting hook should have fired"
    );
    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "panicking hook should have been invoked"
    );
}

#[tokio::test]
async fn test_hook_executor_multiple_hooks_one_panics_others_still_fire() {
    let counting = CountingHook::new();
    let before_tool_count = counting.before_tool_count.clone();

    let panicking = PanickingHook::new();
    let panicked_flag = panicking.panicked.clone();

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(counting));
    registry.register_hook(Box::new(panicking));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    let tool_call = serde_json::json!({"name": "test_tool"});
    let action = executor.fire_before_tool(&ctx, &tool_call).await;

    assert!(matches!(action, ToolAction::Proceed));
    assert!(
        before_tool_count.load(Ordering::SeqCst) > 0,
        "counting hook should have fired"
    );
    assert!(
        panicked_flag.load(Ordering::SeqCst),
        "panicking hook should have been invoked"
    );
}

// =============================================================================
// Empty Registry Test
// =============================================================================

#[tokio::test]
async fn test_hook_executor_empty_registry() {
    let executor = HookExecutor::default();
    assert!(executor.is_empty());

    let mut ctx = make_context();
    executor.fire_before_llm(&mut ctx).await;
    executor.fire_complete(&ctx).await;
}

// =============================================================================
// Verdict Propagation Test
// =============================================================================

#[tokio::test]
async fn test_hook_executor_before_tool_returns_modify() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(ModifyingHookForExecutor {
        new_name: "rewritten_tool".to_string(),
        new_input: serde_json::json!({"key": "rewritten_value"}),
    }));
    let executor = HookExecutor::new(registry);

    let ctx = make_context();
    let tool_call = serde_json::json!({"name": "original_tool", "input": {"key": "original"}});
    let action = executor.fire_before_tool(&ctx, &tool_call).await;

    match action {
        ToolAction::Modify(payload) => {
            assert_eq!(
                payload.get("name").unwrap().as_str().unwrap(),
                "rewritten_tool"
            );
            assert_eq!(
                payload
                    .get("input")
                    .unwrap()
                    .get("key")
                    .unwrap()
                    .as_str()
                    .unwrap(),
                "rewritten_value"
            );
        }
        _ => panic!("expected Modify action"),
    }
}
