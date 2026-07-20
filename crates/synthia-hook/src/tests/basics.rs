//! Basic registry / conversion tests.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

#[allow(deprecated)]
use crate::traits::AgentHook;
use crate::{
    HookRegistry,
    traits::{AgentContext, ToolAction, ToolCall},
};

#[derive(Debug)]
struct TestHook {
    call_count: AtomicUsize,
}

impl TestHook {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
#[allow(deprecated)]
impl AgentHook for TestHook {
    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), synthia_core::Error> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn hook_registry_register() {
    let registry = HookRegistry::new();
    let _handle = registry.register_hook(Box::new(TestHook::new()));
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
}

#[tokio::test]
async fn hook_registry_unregister() {
    let registry = HookRegistry::new();
    let handle = registry.register_hook(Box::new(TestHook::new()));
    assert!(registry.unregister_by_handle(&handle));
    assert!(registry.is_empty());
    assert!(!registry.unregister_by_handle(&handle));
}

#[tokio::test]
async fn fire_hooks_in_order() {
    let registry = HookRegistry::new();
    let hook1 = Box::new(TestHook::new());
    let hook2 = Box::new(TestHook::new());
    registry.register_hook(hook1);
    registry.register_hook(hook2);

    let mut ctx =
        AgentContext::new("session-1".to_string(), "turn-1".to_string());
    registry.fire_before_llm(&mut ctx).await.unwrap();

    assert_eq!(registry.len(), 2);
}

#[tokio::test]
async fn tool_action_variants() {
    assert_eq!(ToolAction::Proceed, ToolAction::Proceed);
    assert_eq!(ToolAction::Skip, ToolAction::Skip);
    assert_ne!(ToolAction::Proceed, ToolAction::Skip);

    let modified =
        ToolAction::Modify(serde_json::json!({"name": "modified_tool"}));
    assert_ne!(modified, ToolAction::Proceed);
}

#[test]
fn agent_context_creation() {
    let ctx = AgentContext::new("s1".to_string(), "t1".to_string());
    assert_eq!(ctx.session_id, "s1");
    assert_eq!(ctx.turn_id, "t1");
    assert_eq!(ctx.iteration, 0);
    assert!(ctx.messages.is_empty());
    assert!(ctx.metadata.is_empty());
}

#[test]
fn toolcall_from_value() {
    let v = serde_json::json!({
        "id": "call-1",
        "name": "read_file",
        "input": {"path": "/tmp/test"}
    });
    let tc = ToolCall::from_value(&v).unwrap();
    assert_eq!(tc.id, "call-1");
    assert_eq!(tc.name, "read_file");
    assert_eq!(tc.input.get("path").unwrap().as_str().unwrap(), "/tmp/test");

    let v2 = serde_json::json!({"name": "write_file", "input": {}});
    let tc2 = ToolCall::from_value(&v2).unwrap();
    assert_eq!(tc2.id, "");
}

#[test]
fn toolcall_to_value_roundtrip() {
    let tc = ToolCall {
        id: "call-2".to_string(),
        name: "run_cmd".to_string(),
        input: serde_json::json!({"cmd": "ls"}),
    };
    let v = tc.to_value();
    let tc2 = ToolCall::from_value(&v).unwrap();
    assert_eq!(tc, tc2);
}

#[test]
fn message_roundtrip() {
    let msg = synthia_provider::types::Message::user("hello");
    let v = crate::message_to_value(&msg);
    let msg2 = crate::message_from_value(&v).unwrap();
    assert_eq!(msg, msg2);
}
