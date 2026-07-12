use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::{
    FileChangeEvent,
    Tool,
    types::{ToolInput, ToolOutput},
};

struct TestTool {
    name: &'static str,
    description: &'static str,
    params: serde_json::Value,
    requires_permission: bool,
    is_hidden: bool,
    is_concurrency_safe: bool,
    call_count: std::sync::atomic::AtomicUsize,
}

impl TestTool {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            description: "A test tool",
            params: json!({"type": "object"}),
            requires_permission: false,
            is_hidden: false,
            is_concurrency_safe: true,
            call_count: Default::default(),
        }
    }

    fn with_permission(mut self, val: bool) -> Self {
        self.requires_permission = val;
        self
    }

    fn with_hidden(mut self, val: bool) -> Self {
        self.is_hidden = val;
        self
    }

    fn with_concurrency_safe(mut self, val: bool) -> Self {
        self.is_concurrency_safe = val;
        self
    }
}

#[async_trait]
impl Tool for TestTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.params.clone()
    }

    fn requires_permission(&self) -> bool {
        self.requires_permission
    }

    fn is_hidden(&self) -> bool {
        self.is_hidden
    }

    fn is_concurrency_safe(&self) -> bool {
        self.is_concurrency_safe
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ToolOutput::text(format!("called {}", self.name))
    }

    async fn call_with_sandbox(
        &self,
        input: ToolInput,
        _sandbox_attempt: &synthia_sandbox::SandboxAttempt,
        _token: &tokio_util::sync::CancellationToken,
    ) -> ToolOutput {
        self.call(input).await
    }

    async fn call_with_progress(
        &self,
        input: ToolInput,
        _on_event: Arc<dyn Fn(FileChangeEvent) + Send + Sync>,
        _token: &tokio_util::sync::CancellationToken,
    ) -> ToolOutput {
        self.call(input).await
    }
}

#[test]
fn tool_name_and_description() {
    let tool = TestTool::new("test_tool");
    assert_eq!(tool.name(), "test_tool");
    assert_eq!(tool.description(), "A test tool");
}

#[test]
fn tool_parameters() {
    let tool = TestTool::new("test_tool");
    let params = tool.parameters();
    assert_eq!(params["type"], "object");
}

#[test]
fn tool_default_requires_permission() {
    let tool = TestTool::new("test_tool");
    assert!(!tool.requires_permission());
}

#[test]
fn tool_explicit_requires_permission() {
    let tool = TestTool::new("test_tool").with_permission(true);
    assert!(tool.requires_permission());
}

#[test]
fn tool_default_is_hidden() {
    let tool = TestTool::new("test_tool");
    assert!(!tool.is_hidden());
}

#[test]
fn tool_explicit_is_hidden() {
    let tool = TestTool::new("test_tool").with_hidden(true);
    assert!(tool.is_hidden());
}

#[test]
fn tool_default_is_concurrency_safe() {
    let tool = TestTool::new("test_tool");
    assert!(tool.is_concurrency_safe());
}

#[test]
fn tool_explicit_not_concurrency_safe() {
    let tool = TestTool::new("test_tool").with_concurrency_safe(false);
    assert!(!tool.is_concurrency_safe());
}

#[tokio::test]
async fn tool_call_increments_count() {
    let tool = TestTool::new("counted_tool");
    assert_eq!(tool.call_count.load(std::sync::atomic::Ordering::SeqCst), 0);

    let input = ToolInput {
        name: "counted_tool".to_string(),
        input: serde_json::json!({}),
        context: crate::types::ToolExecutionContext::new(
            "session".to_string(),
            std::path::PathBuf::from("/"),
        ),
    };
    tool.call(input).await;

    assert_eq!(tool.call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn tool_trait_object_sends() {
    fn _assert<T: Send + Sync>() {}
    _assert::<Box<dyn Tool>>();
}

// Tests for the 3 new trait methods added by the
// `tool-abstraction-and-extensibility` change (Phase 1, Task 1.1).

#[test]
fn tool_default_execution_mode_is_parallel() {
    let tool = TestTool::new("any_tool");
    assert_eq!(
        tool.execution_mode(),
        crate::traits::ExecutionMode::Parallel
    );
}

#[test]
fn tool_default_is_user_invocable_is_true() {
    let tool = TestTool::new("any_tool");
    assert!(tool.is_user_invocable());
}

#[test]
fn tool_default_output_preserves_raw() {
    let tool = TestTool::new("any_tool");
    let raw = serde_json::json!({"x": 1, "y": [1, 2, 3]});
    let out = tool.output(raw.clone());
    assert!(out.is_text());
    assert!(out.metadata.is_empty());
    assert!(out.truncated_by.is_none());
    // The textual content should round-trip back to the original JSON
    // via `ToolOutput::from_raw`'s `raw.to_string()` strategy.
    assert!(out.content[0].text().unwrap().contains("\"x\""));
}

struct SequentialTestTool;

#[async_trait]
impl Tool for SequentialTestTool {
    fn name(&self) -> &str {
        "seq_tool"
    }

    fn description(&self) -> &str {
        "test sequential tool"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({})
    }

    fn execution_mode(&self) -> crate::traits::ExecutionMode {
        crate::traits::ExecutionMode::Sequential
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text("seq")
    }
}

#[test]
fn tool_can_override_execution_mode_to_sequential() {
    let tool = SequentialTestTool;
    assert_eq!(
        tool.execution_mode(),
        crate::traits::ExecutionMode::Sequential
    );
}

struct HiddenButInvocableTool;

#[async_trait]
impl Tool for HiddenButInvocableTool {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "loads a skill"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({})
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_user_invocable(&self) -> bool {
        // Hidden from help listings but still exposed to the LLM.
        true
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text("loaded")
    }
}

#[test]
fn load_skill_is_hidden_but_user_invocable() {
    let tool = HiddenButInvocableTool;
    assert!(tool.is_hidden());
    assert!(tool.is_user_invocable());
}
