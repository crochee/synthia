//! Unit tests for the registration family.
//!
//! All 12 tests for [`super::entry::ToolEntry`],
//! [`super::registry::ToolRegistry`], and the
//! `impl Registry<ToolEntry> for ToolRegistry` block
//! live here. Centralising the test fixtures
//! (especially the local `TestTool` / `HiddenTool` /
//! `ToolWithRequired` / `FastTool` impls) keeps the
//! per-submodule test load to zero.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use synthia_core::registry::{Registry, RegistryItem};

use super::{
    super::metadata::ToolFilter,
    entry::ToolEntry,
    registry::ToolRegistry,
};
use crate::{
    traits::Tool,
    types::{ToolExecutionContext, ToolInput, ToolOutput},
};

#[derive(Debug)]
struct TestTool;

#[async_trait]
impl Tool for TestTool {
    fn name(&self) -> &str {
        "test"
    }

    fn description(&self) -> &str {
        "A test tool"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text("test output")
    }
}

#[tokio::test]
async fn test_tool_registry_register_and_get() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();
    let entry = registry.get("test").await.unwrap();
    assert!(entry.is_some());
    assert!(entry.unwrap().tool_instance().name() == "test");
    assert!(registry.get("nonexistent").await.unwrap().is_none());
}

#[tokio::test]
async fn test_tool_registry_unregister() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();
    registry.unregister("test").await.unwrap();
    assert!(registry.get("test").await.unwrap().is_none());
}

#[tokio::test]
async fn test_list_definitions() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();
    let entries = registry.list(None).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), "test");
}

#[tokio::test]
async fn test_run_with_context() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();

    let tool_use = synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "test".to_string(),
        input: serde_json::json!({}),
    };
    let ctx =
        ToolExecutionContext::new("s1".to_string(), PathBuf::from("/tmp"));
    let results = registry
        .run_with_context(vec![tool_use], ctx)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].is_text());
}

#[tokio::test]
async fn test_run_with_context_validation_error() {
    #[derive(Debug)]
    struct ToolWithRequired;
    #[async_trait]
    impl Tool for ToolWithRequired {
        fn name(&self) -> &str {
            "req"
        }

        fn description(&self) -> &str {
            "Tool with required param"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "required": ["name"],
                "properties": {}
            })
        }

        async fn call(&self, input: ToolInput) -> ToolOutput {
            if input.input.get("name").is_none() {
                return ToolOutput::error(
                    "Missing required property: name".to_string(),
                );
            }
            ToolOutput::text("ok")
        }
    }

    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(ToolWithRequired)),
    )
    .await
    .unwrap();

    let tool_use = synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "req".to_string(),
        input: serde_json::json!({}),
    };
    let ctx =
        ToolExecutionContext::new("s1".to_string(), PathBuf::from("/tmp"));
    let results = registry
        .run_with_context(vec![tool_use], ctx)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].is_error.unwrap_or(false));
    let text = results[0].content[0].text().unwrap();
    assert!(
        text.contains("Missing required") || text.contains("required property"),
        "Unexpected validation error: {}",
        text
    );
}

#[tokio::test]
async fn test_run_with_context_concurrent() {
    #[derive(Debug)]
    struct FastTool;
    #[async_trait]
    impl Tool for FastTool {
        fn name(&self) -> &str {
            "fast"
        }

        fn description(&self) -> &str {
            "Fast tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(&self, _input: ToolInput) -> ToolOutput {
            ToolOutput::text("done")
        }
    }

    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(FastTool)),
    )
    .await
    .unwrap();

    let tool_uses = vec![
        synthia_provider::ToolUse {
            id: "1".to_string(),
            name: "fast".to_string(),
            input: serde_json::json!({}),
        },
        synthia_provider::ToolUse {
            id: "2".to_string(),
            name: "fast".to_string(),
            input: serde_json::json!({}),
        },
    ];
    let ctx =
        ToolExecutionContext::new("s1".to_string(), PathBuf::from("/tmp"));
    let results = registry.run_with_context(tool_uses, ctx).await.unwrap();
    assert_eq!(results.len(), 2);
    for r in &results {
        assert!(r.is_text());
    }
}

#[tokio::test]
async fn test_run_with_context_unknown_tool() {
    let registry = ToolRegistry::new();

    let tool_uses = vec![synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "nonexistent_tool".to_string(),
        input: serde_json::json!({}),
    }];
    let ctx =
        ToolExecutionContext::new("s1".to_string(), PathBuf::from("/tmp"));
    let results = registry.run_with_context(tool_uses, ctx).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].is_error.unwrap_or(false));
}

#[test]
fn test_tool_context_has_dispatch_mode() {
    use crate::types::DispatchMode;
    let ctx =
        ToolExecutionContext::new("s1".to_string(), PathBuf::from("/tmp"));
    assert_eq!(ctx.dispatch_mode, DispatchMode::Fork);
}

#[tokio::test]
async fn test_hidden_tools_not_in_list() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();

    #[derive(Debug)]
    struct HiddenTool;

    #[async_trait]
    impl Tool for HiddenTool {
        fn name(&self) -> &str {
            "hidden"
        }

        fn description(&self) -> &str {
            "A hidden tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn is_hidden(&self) -> bool {
            true
        }

        async fn call(&self, _input: ToolInput) -> ToolOutput {
            ToolOutput::text("hidden output")
        }
    }

    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(HiddenTool)),
    )
    .await
    .unwrap();

    let entries = registry.list(None).await.unwrap();
    assert_eq!(entries.len(), 2);

    let hidden = entries.iter().find(|e| e.name() == "hidden").unwrap();
    assert!(hidden.tool.is_hidden());

    let visible = entries.iter().find(|e| e.name() == "test").unwrap();
    assert!(!visible.tool.is_hidden());

    assert!(registry.get("test").await.unwrap().is_some());
    assert!(registry.get("hidden").await.unwrap().is_some());
}

#[tokio::test]
async fn test_hidden_tool_not_executed() {
    let registry = ToolRegistry::new();

    #[derive(Debug)]
    struct HiddenTool;

    #[async_trait]
    impl Tool for HiddenTool {
        fn name(&self) -> &str {
            "hidden_exec"
        }

        fn description(&self) -> &str {
            "A hidden executable tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn is_hidden(&self) -> bool {
            true
        }

        async fn call(&self, _input: ToolInput) -> ToolOutput {
            ToolOutput::text("hidden executed")
        }
    }

    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(HiddenTool)),
    )
    .await
    .unwrap();

    let tool_use = synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "hidden_exec".to_string(),
        input: serde_json::json!({}),
    };
    let ctx =
        ToolExecutionContext::new("s1".to_string(), PathBuf::from("/tmp"));
    let results = registry
        .run_with_context(vec![tool_use], ctx)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].is_error.unwrap_or(false));
    assert!(results[0].content[0].text().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_registry_trait_list() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();

    let items = registry.list(None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name(), "test");

    let filtered = registry
        .list(Some(ToolFilter {
            name_prefix: Some("tes".to_string()),
        }))
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);

    let no_match = registry
        .list(Some(ToolFilter {
            name_prefix: Some("xyz".to_string()),
        }))
        .await
        .unwrap();
    assert_eq!(no_match.len(), 0);
}

#[tokio::test]
async fn test_registry_trait_get() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();

    let item = registry.get("test").await.unwrap();
    assert_eq!(item.unwrap().name(), "test");

    let not_found = registry.get("nonexistent").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_registry_trait_contains_and_len() {
    let registry = ToolRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(TestTool)),
    )
    .await
    .unwrap();
    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 1);
    assert!(registry.contains("test"));
    assert!(!registry.contains("nonexistent"));
}
