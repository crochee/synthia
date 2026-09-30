use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use synthia_core::{RegistryItem, registry::Registry};
use synthia_tool::{Context, Tool, ToolEntry, ToolOutput, ToolRegistry};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MockTool {
    name: String,
    description: String,
}

impl MockTool {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for MockTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text(format!("Called {}", self.name))
    }
}

#[tokio::test]
async fn test_tool_registry_register_and_get() {
    let registry = ToolRegistry::new();
    let tool = MockTool::new("test_tool", "A test tool");

    registry.register_entry(ToolEntry::new(Arc::new(tool)));

    let snapshots = registry.snapshot();
    assert!(snapshots.iter().any(|s| s.name == "test_tool"));
    assert!(snapshots.iter().all(|s| s.name != "nonexistent"));

    let retrieved = registry.get("test_tool").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name(), "test_tool");
}

#[tokio::test]
async fn test_tool_registry_list_tools() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(MockTool::new(
        "tool1",
        "First tool",
    ))));
    registry.register_entry(ToolEntry::new(Arc::new(MockTool::new(
        "tool2",
        "Second tool",
    ))));

    let tools = registry.list(None).await.unwrap();
    assert_eq!(tools.len(), 2);
}

#[tokio::test]
async fn test_tool_registry_empty() {
    let registry = ToolRegistry::new();
    assert!(registry.snapshot().is_empty());
    assert!(registry.get("anything").await.unwrap().is_none());
}

#[tokio::test]
async fn test_tool_call() {
    let tool = MockTool::new("mock", "Mock tool");
    let context =
        Context::new("test-session".to_string(), PathBuf::from("/tmp"));
    let result = tool.call(json!({}), &context).await;

    assert!(result.is_text());
    assert_eq!(result.content.len(), 1);
}
