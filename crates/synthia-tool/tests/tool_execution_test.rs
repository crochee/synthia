#![allow(deprecated)]
use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use synthia_tool::{Tool, ToolEntry, ToolInput, ToolOutput, ToolRegistry};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MockTool {
    name: String,
    description: String,
    should_error: bool,
    error_message: Option<String>,
}

impl MockTool {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            should_error: false,
            error_message: None,
        }
    }

    fn with_error(name: &str, description: &str, error_message: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            should_error: true,
            error_message: Some(error_message.to_string()),
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
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Input string"
                }
            }
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        if self.should_error {
            ToolOutput::error(
                self.error_message
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string()),
            )
        } else {
            let input_str = input
                .input
                .get("input")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            ToolOutput::text(format!("{} returned: {}", self.name, input_str))
        }
    }
}

fn make_context() -> synthia_tool::ToolExecutionContext {
    synthia_tool::ToolExecutionContext::new(
        "test-session".to_string(),
        PathBuf::from("/tmp"),
    )
}

fn make_tool_use(name: &str, input: Value) -> synthia_provider::ToolUse {
    synthia_provider::ToolUse {
        id: "test-id".to_string(),
        name: name.to_string(),
        input,
    }
}

// ============ Successful tool call tests ============

#[tokio::test]
async fn test_successful_tool_call() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "success_tool",
        "A successful tool",
    ))));

    let tool_uses =
        vec![make_tool_use("success_tool", json!({"input": "hello"}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_text());
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    assert!(text.contains("success_tool"));
    assert!(text.contains("hello"));
}

#[tokio::test]
async fn test_multiple_successful_tool_calls() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "tool1",
        "First tool",
    ))));
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "tool2",
        "Second tool",
    ))));
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "tool3",
        "Third tool",
    ))));

    let tool_uses = vec![
        make_tool_use("tool1", json!({})),
        make_tool_use("tool2", json!({})),
        make_tool_use("tool3", json!({})),
    ];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 3);
    for output in &outputs {
        assert!(output.is_text());
    }
}

#[tokio::test]
async fn test_tool_call_with_empty_input() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "empty_input_tool",
        "Tool with empty input",
    ))));

    let tool_uses = vec![make_tool_use("empty_input_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_text());
}

// ============ Tool error handling tests ============

#[tokio::test]
async fn test_tool_error_handling() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::with_error(
        "error_tool",
        "Error tool",
        "Something went wrong",
    ))));

    let tool_uses = vec![make_tool_use("error_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    assert!(text.contains("Something went wrong"));
}

#[tokio::test]
async fn test_multiple_tools_with_one_error() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "good1",
        "Good tool 1",
    ))));
    registry.register(ToolEntry::new(Arc::new(MockTool::with_error(
        "bad",
        "Bad tool",
        "I always fail",
    ))));
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "good2",
        "Good tool 2",
    ))));

    let tool_uses = vec![
        make_tool_use("good1", json!({})),
        make_tool_use("bad", json!({})),
        make_tool_use("good2", json!({})),
    ];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 3);
    assert!(outputs[0].is_text());
    assert!(outputs[1].is_error == Some(true));
    assert!(outputs[2].is_text());
}

#[tokio::test]
async fn test_tool_returns_error_content() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::with_error(
        "error_content",
        "Error content tool",
        "File not found: /tmp/nonexistent.txt",
    ))));

    let tool_uses = vec![make_tool_use("error_content", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    assert!(text.contains("File not found"));
}

// ============ Tool not found tests ============

#[tokio::test]
async fn test_tool_not_found() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "existing",
        "Existing tool",
    ))));

    let tool_uses = vec![make_tool_use("nonexistent", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    assert!(text.contains("not found"));
}

#[tokio::test]
async fn test_multiple_tools_with_nonexistent() {
    let registry = ToolRegistry::new();
    registry
        .register(ToolEntry::new(Arc::new(MockTool::new("tool1", "Tool 1"))));

    let tool_uses = vec![
        make_tool_use("tool1", json!({})),
        make_tool_use("nonexistent1", json!({})),
        make_tool_use("nonexistent2", json!({})),
    ];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 3);
    assert!(outputs[0].is_text());
    assert!(outputs[1].is_error == Some(true));
    assert!(outputs[2].is_error == Some(true));
}

#[tokio::test]
async fn test_all_tools_not_found() {
    let registry = ToolRegistry::new();

    let tool_uses = vec![
        make_tool_use("missing1", json!({})),
        make_tool_use("missing2", json!({})),
    ];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 2);
    for output in &outputs {
        assert!(output.is_error == Some(true));
    }
}

// ============ Parameter validation tests ============

#[tokio::test]
async fn test_parameter_validation_string() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "param_tool",
        "Parameter tool",
    ))));

    let tool_uses =
        vec![make_tool_use("param_tool", json!({"input": "test_value"}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_text());
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    assert!(text.contains("test_value"));
}

#[tokio::test]
async fn test_parameter_validation_missing_optional() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "optional_param_tool",
        "Optional parameter tool",
    ))));

    let tool_uses = vec![make_tool_use("optional_param_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    // Missing optional parameter should not cause error
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_text());
}

#[tokio::test]
async fn test_parameter_validation_complex_json() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new(
        "complex_param_tool",
        "Complex parameter tool",
    ))));

    let tool_uses = vec![make_tool_use(
        "complex_param_tool",
        json!({
            "input": "value",
            "extra": {
                "nested": ["array", "values"],
                "number": 42
            }
        }),
    )];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_text());
}

// ============ Empty inputs tests ============

#[tokio::test]
async fn test_empty_tool_uses_list() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockTool::new("tool", "Tool"))));

    let tool_uses: Vec<synthia_provider::ToolUse> = vec![];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();

    assert!(outputs.is_empty());
}

#[tokio::test]
async fn test_context_passed_to_tool() {
    use std::sync::Mutex;

    let context_received = Arc::new(Mutex::new(None));

    #[derive(Debug, Clone)]
    struct ContextCapturingTool {
        name: String,
        captured: Arc<Mutex<Option<synthia_tool::ToolExecutionContext>>>,
    }

    impl ContextCapturingTool {
        fn new(
            captured: Arc<Mutex<Option<synthia_tool::ToolExecutionContext>>>,
        ) -> Self {
            Self {
                name: "context_tool".to_string(),
                captured,
            }
        }
    }

    #[async_trait]
    impl Tool for ContextCapturingTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Context capturing tool"
        }

        fn parameters(&self) -> Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn call(&self, input: ToolInput) -> ToolOutput {
            *self.captured.lock().unwrap() = Some(input.context);
            ToolOutput::text("captured")
        }
    }

    let captured = context_received.clone();
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(ContextCapturingTool::new(
        captured,
    ))));

    let mut context = make_context();
    context.caller_agent = "test-agent".to_string();

    let tool_uses = vec![make_tool_use("context_tool", json!({}))];

    registry
        .run_with_context(tool_uses, context.clone())
        .await
        .unwrap();

    let captured_context = context_received.lock().unwrap();
    assert!(captured_context.is_some());
    assert_eq!(
        captured_context.as_ref().unwrap().session_id,
        "test-session"
    );
    assert_eq!(
        captured_context.as_ref().unwrap().caller_agent,
        "test-agent"
    );
}
