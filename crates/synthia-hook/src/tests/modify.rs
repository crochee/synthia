//! `ToolAction::Modify` shape tests.
//!
//! Verifies that a hook can return only a `name`, only an `input`,
//! or both, and that the unmodified field is left absent from the
//! returned payload.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::Error;

use crate::{
    HookRegistry,
    traits::{AgentContext, AgentHook, ToolAction},
};

#[derive(Debug)]
struct ModifyHookNameOnly {
    new_name: String,
}

#[async_trait]
impl AgentHook for ModifyHookNameOnly {
    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        Ok(ToolAction::Modify(serde_json::json!({
            "name": self.new_name
        })))
    }
}

#[derive(Debug)]
struct ModifyHookInputOnly {
    new_input: serde_json::Value,
}

#[async_trait]
impl AgentHook for ModifyHookInputOnly {
    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        Ok(ToolAction::Modify(serde_json::json!({
            "input": self.new_input
        })))
    }
}

#[derive(Debug)]
struct ModifyHookBoth {
    new_name: String,
    new_input: serde_json::Value,
}

#[async_trait]
impl AgentHook for ModifyHookBoth {
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
}

#[tokio::test]
async fn modify_hook_returns_name_and_input_fields() {
    let original_call = serde_json::json!({
        "id": "call-1",
        "name": "read_file",
        "input": {"path": "/etc/passwd"}
    });

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(ModifyHookBoth {
        new_name: "safe_read".to_string(),
        new_input: serde_json::json!({"path": "/tmp/safe.txt"}),
    }));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    let action = registry
        .fire_before_tool(&ctx, &original_call)
        .await
        .unwrap();

    match action {
        ToolAction::Modify(payload) => {
            let name = payload.get("name").and_then(|v| v.as_str()).unwrap();
            let input = payload.get("input").unwrap();
            assert_eq!(name, "safe_read");
            assert_eq!(
                input.get("path").unwrap().as_str().unwrap(),
                "/tmp/safe.txt"
            );
        }
        _ => panic!("expected Modify action"),
    }
}

#[tokio::test]
async fn modify_hook_name_only_preserves_input_field() {
    let original_call = serde_json::json!({
        "id": "call-1",
        "name": "read_file",
        "input": {"path": "/etc/passwd"}
    });

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(ModifyHookNameOnly {
        new_name: "safe_read".to_string(),
    }));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    let action = registry
        .fire_before_tool(&ctx, &original_call)
        .await
        .unwrap();

    match action {
        ToolAction::Modify(payload) => {
            assert_eq!(
                payload.get("name").unwrap().as_str().unwrap(),
                "safe_read"
            );
            assert!(
                payload.get("input").is_none(),
                "input field should not be present when not modified"
            );
        }
        _ => panic!("expected Modify action"),
    }
}

#[tokio::test]
async fn modify_hook_input_only_preserves_name_field() {
    let original_call = serde_json::json!({
        "id": "call-1",
        "name": "read_file",
        "input": {"path": "/etc/passwd"}
    });

    let registry = HookRegistry::new();
    registry.register_hook(Box::new(ModifyHookInputOnly {
        new_input: serde_json::json!({"path": "/tmp/safe.txt"}),
    }));

    let ctx = AgentContext::new("session-1".to_string(), "turn-1".to_string());
    let action = registry
        .fire_before_tool(&ctx, &original_call)
        .await
        .unwrap();

    match action {
        ToolAction::Modify(payload) => {
            assert!(
                payload.get("name").is_none(),
                "name field should not be present when not modified"
            );
            assert_eq!(
                payload
                    .get("input")
                    .unwrap()
                    .get("path")
                    .unwrap()
                    .as_str()
                    .unwrap(),
                "/tmp/safe.txt"
            );
        }
        _ => panic!("expected Modify action"),
    }
}

#[allow(dead_code)]
fn _suppress_unused_arc(_: Arc<()>) {}
