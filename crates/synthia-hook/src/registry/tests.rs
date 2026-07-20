//! Unit tests for the `registry` module family.
//!
//! Coverage map (7 tests):
//!
//! - `register_hook` / `unregister_by_handle` / `len` /
//!   `is_empty` / `contains`: 2 tests
//!   ([`test_hook_registry_register`],
//!   [`test_hook_registry_unregister_by_handle`]).
//! - `Registry<HookInfo>` trait: 5 tests
//!   ([`test_registry_trait_list`],
//!   [`test_registry_trait_get`],
//!   [`test_registry_trait_unregister`],
//!   [`test_registry_trait_unregister_not_found`]).

use async_trait::async_trait;
use synthia_core::{Error, Registry};

use super::*;
#[allow(deprecated)]
use crate::traits::AgentHook;
use crate::traits::{AgentContext, ToolAction};

#[derive(Debug)]
struct TestHook;

#[async_trait]
#[allow(deprecated)]
impl AgentHook for TestHook {
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

    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        Ok(ToolAction::Proceed)
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

#[test]
fn test_hook_registry_register() {
    let registry = HookRegistry::new();
    let _handle = registry.register_hook(Box::new(TestHook));
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
    assert!(registry.contains("TestHook"));
}

#[test]
fn test_hook_registry_unregister_by_handle() {
    let registry = HookRegistry::new();
    let handle = registry.register_hook(Box::new(TestHook));
    assert!(registry.unregister_by_handle(&handle));
    assert!(registry.is_empty());
}

#[tokio::test]
async fn test_registry_trait_list() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(TestHook));

    let items = registry.list(None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "TestHook");
}

#[tokio::test]
async fn test_registry_trait_get() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(TestHook));

    let item = registry.get("TestHook").await.unwrap();
    assert_eq!(item.unwrap().name, "TestHook");

    let not_found = registry.get("nonexistent").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_registry_trait_unregister() {
    let registry = HookRegistry::new();
    registry.register_hook(Box::new(TestHook));

    assert!(registry.contains("TestHook"));
    registry.unregister("TestHook").await.unwrap();
    assert!(!registry.contains("TestHook"));
}

#[tokio::test]
async fn test_registry_trait_unregister_not_found() {
    let registry = HookRegistry::new();
    let result = registry.unregister("nonexistent").await;
    assert!(result.is_err());
}
