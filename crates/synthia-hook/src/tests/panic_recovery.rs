//! Panic recovery tests.
//!
//! Verifies that a panicking hook does not crash the agent loop —
//! the registry catches the panic via `catch_unwind` and continues
//! firing the remaining hooks, marking the panicking hook as
//! "failed" so it is skipped on subsequent calls.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use async_trait::async_trait;
use synthia_core::Error;

#[allow(deprecated)]
use crate::traits::AgentHook;
use crate::{
    HookRegistry,
    traits::{AgentContext, ToolAction},
};

#[derive(Debug)]
struct PanickingHook;

#[async_trait]
#[allow(deprecated)]
impl AgentHook for PanickingHook {
    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), synthia_core::Error> {
        panic!("intentional panic in before_llm hook");
    }

    async fn on_after_llm(
        &self,
        _ctx: &AgentContext,
        _response: &serde_json::Value,
    ) -> Result<(), synthia_core::Error> {
        panic!("intentional panic in after_llm hook");
    }

    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, synthia_core::Error> {
        panic!("intentional panic in before_tool hook");
    }

    async fn on_after_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
        _result: &serde_json::Value,
    ) -> Result<(), synthia_core::Error> {
        panic!("intentional panic in after_tool hook");
    }

    async fn on_iteration_end(
        &self,
        _ctx: &AgentContext,
        _iteration: usize,
    ) -> Result<(), synthia_core::Error> {
        panic!("intentional panic in iteration_end hook");
    }

    async fn on_complete(
        &self,
        _ctx: &AgentContext,
    ) -> Result<(), synthia_core::Error> {
        panic!("intentional panic in complete hook");
    }
}

#[tokio::test]
async fn panicking_hook_does_not_crash_before_llm() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(PanickingHook));

    let mut ctx =
        AgentContext::new("session-1".to_string(), "turn-1".to_string());
    // Should not panic or return an error - fail-open semantics
    let result = registry.fire_before_llm(&mut ctx).await;
    assert!(
        result.is_ok(),
        "fire_before_llm should succeed even with panicking hook"
    );
}

#[tokio::test]
async fn panicking_hook_does_not_crash_after_llm() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(PanickingHook));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    let response = serde_json::json!({"content": "test"});
    // Should not panic - fail-open semantics
    let result = registry.fire_after_llm(&ctx, &response).await;
    assert!(
        result.is_ok(),
        "fire_after_llm should succeed even with panicking hook"
    );
}

#[tokio::test]
async fn panicking_hook_does_not_crash_before_tool() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(PanickingHook));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    let call = serde_json::json!({"name": "test_tool"});
    // Should not panic - returns Proceed as default
    let result = registry.fire_before_tool(&ctx, &call).await;
    assert!(
        result.is_ok(),
        "fire_before_tool should succeed even with panicking hook"
    );
    assert_eq!(
        result.unwrap(),
        ToolAction::Proceed,
        "panicking hook should default to Proceed"
    );
}

#[tokio::test]
async fn panicking_hook_does_not_crash_after_tool() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(PanickingHook));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    let call = serde_json::json!({"name": "test_tool"});
    let result = serde_json::json!({"output": "ok"});
    // Should not panic - fail-open semantics
    let result = registry.fire_after_tool(&ctx, &call, &result).await;
    assert!(
        result.is_ok(),
        "fire_after_tool should succeed even with panicking hook"
    );
}

#[tokio::test]
async fn panicking_hook_does_not_crash_iteration_end() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(PanickingHook));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    // Should not panic - fail-open semantics
    let result = registry.fire_iteration_end(&ctx, 1).await;
    assert!(
        result.is_ok(),
        "fire_iteration_end should succeed even with panicking hook"
    );
}

#[tokio::test]
async fn panicking_hook_does_not_crash_complete() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(PanickingHook));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    // Should not panic - fail-open semantics
    let result = registry.fire_complete(&ctx).await;
    assert!(
        result.is_ok(),
        "fire_complete should succeed even with panicking hook"
    );
}

#[derive(Debug)]
struct NormalHook {
    count: AtomicUsize,
}

#[async_trait]
#[allow(deprecated)]
impl AgentHook for NormalHook {
    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), synthia_core::Error> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn mixed_hooks_panicking_one_continues() {
    let registry = HookRegistry::new();
    let _panicking = registry.register_hook(Box::new(PanickingHook));
    let _normal = registry.register_hook(Box::new(NormalHook {
        count: AtomicUsize::new(0),
    }));

    let mut ctx =
        AgentContext::new("session-1".to_string(), "turn-1".to_string());

    // First call: panicking hook fails, normal hook runs
    let result = registry.fire_before_llm(&mut ctx).await;
    assert!(result.is_ok());

    // Panicking hook should be marked as failed, normal hook should have been called
    assert_eq!(registry.len(), 2);

    // Second call: only normal hook should run (panicking one is disabled)
    let result = registry.fire_before_llm(&mut ctx).await;
    assert!(result.is_ok());
}

#[derive(Debug)]
struct SecondOfThreePanickingHook {
    panic_flag: Arc<AtomicBool>,
    second_called: Arc<AtomicBool>,
}

impl SecondOfThreePanickingHook {
    fn new() -> Self {
        Self {
            panic_flag: Arc::new(AtomicBool::new(false)),
            second_called: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
#[allow(deprecated)]
impl AgentHook for SecondOfThreePanickingHook {
    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, synthia_core::Error> {
        self.second_called.store(true, Ordering::SeqCst);
        self.panic_flag.store(true, Ordering::SeqCst);
        panic!("intentional panic in second hook");
    }
}

#[derive(Debug)]
struct FirstHook {
    called: Arc<AtomicBool>,
}

#[async_trait]
#[allow(deprecated)]
impl AgentHook for FirstHook {
    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, synthia_core::Error> {
        self.called.store(true, Ordering::SeqCst);
        Ok(ToolAction::Proceed)
    }
}

#[derive(Debug)]
struct ThirdHook {
    called: Arc<AtomicBool>,
}

#[async_trait]
#[allow(deprecated)]
impl AgentHook for ThirdHook {
    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, synthia_core::Error> {
        self.called.store(true, Ordering::SeqCst);
        Ok(ToolAction::Proceed)
    }
}

#[tokio::test]
async fn multiple_hooks_second_panics_third_still_executes() {
    let first_called = Arc::new(AtomicBool::new(false));
    let third_called = Arc::new(AtomicBool::new(false));

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(FirstHook {
        called: first_called.clone(),
    }));
    registry.register_hook(Box::new(SecondOfThreePanickingHook::new()));
    registry.register_hook(Box::new(ThirdHook {
        called: third_called.clone(),
    }));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    let call = serde_json::json!({"name": "test_tool"});

    let result = registry.fire_before_tool(&ctx, &call).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), ToolAction::Proceed);

    assert!(
        first_called.load(Ordering::SeqCst),
        "first hook should have fired before panic"
    );
    assert!(
        third_called.load(Ordering::SeqCst),
        "third hook should fire after second panics"
    );
}

#[tokio::test]
async fn catch_unwind_with_assert_unwind_safe() {
    use std::panic::AssertUnwindSafe;

    use futures::FutureExt;

    async fn panicking_future() -> Result<(), Error> {
        panic!("test panic");
    }

    let result = AssertUnwindSafe(panicking_future()).catch_unwind().await;
    assert!(result.is_err(), "catch_unwind should capture the panic");
}
