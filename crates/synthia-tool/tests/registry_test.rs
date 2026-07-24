use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use synthia_core::{RegistryItem, registry::Registry};
use synthia_tool::{Tool, ToolEntry, ToolInput, ToolOutput, ToolRegistry};

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

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text(format!("Called {}", self.name))
    }
}

#[tokio::test]
async fn test_tool_registry_register_and_get() {
    let registry = ToolRegistry::new();
    let tool = MockTool::new("test_tool", "A test tool");

    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(tool)),
    )
    .await
    .unwrap();

    assert!(registry.contains("test_tool"));
    assert!(!registry.contains("nonexistent"));

    let retrieved = registry.get("test_tool").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name(), "test_tool");
}

#[tokio::test]
async fn test_tool_registry_list_tools() {
    let registry = ToolRegistry::new();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(MockTool::new("tool1", "First tool"))),
    )
    .await
    .unwrap();
    <ToolRegistry as Registry<ToolEntry>>::register(
        &registry,
        ToolEntry::new(Arc::new(MockTool::new("tool2", "Second tool"))),
    )
    .await
    .unwrap();

    let tools = registry.list(None).await.unwrap();
    assert_eq!(tools.len(), 2);
}

#[tokio::test]
async fn test_tool_registry_empty() {
    let registry = ToolRegistry::new();
    assert!(registry.is_empty());
    assert!(!registry.contains("anything"));
    assert!(registry.get("anything").await.unwrap().is_none());
}

#[tokio::test]
async fn test_tool_call() {
    let tool = MockTool::new("mock", "Mock tool");
    let input = ToolInput {
        name: "mock".to_string(),
        input: json!({}),
        context: synthia_tool::ToolExecutionContext::new(
            "test-session".to_string(),
            PathBuf::from("/tmp"),
        ),
    };
    let result = tool.call(input).await;

    assert!(result.is_text());
    assert_eq!(result.content.len(), 1);
}

#[test]
fn test_tool_default_values() {
    let tool = MockTool::new("test", "Test");

    assert!(!tool.requires_permission());
    assert!(!tool.is_hidden());
}
