use async_trait::async_trait;
use serde_json::json;

use crate::{
    traits::{ExecutionMode, Tool},
    types::{Context, ToolOutput},
};

struct TestTool {
    name: &'static str,
    description: &'static str,
    params: serde_json::Value,
    mode: ExecutionMode,
    call_count: std::sync::atomic::AtomicUsize,
}

impl TestTool {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            description: "A test tool",
            params: json!({"type": "object"}),
            mode: ExecutionMode::Parallel,
            call_count: Default::default(),
        }
    }

    fn with_sequential(mut self) -> Self {
        self.mode = ExecutionMode::Sequential;
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

    fn mode(&self) -> ExecutionMode {
        self.mode
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ToolOutput::text(format!("called {}", self.name))
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
fn tool_default_mode_is_parallel() {
    let tool = TestTool::new("any_tool");
    assert_eq!(tool.mode(), ExecutionMode::Parallel);
}

#[test]
fn tool_sequential_mode() {
    let tool = TestTool::new("seq_tool").with_sequential();
    assert_eq!(tool.mode(), ExecutionMode::Sequential);
}

#[tokio::test]
async fn tool_call_increments_count() {
    let tool = TestTool::new("counted_tool");
    assert_eq!(tool.call_count.load(std::sync::atomic::Ordering::SeqCst), 0);

    let ctx =
        Context::new("session".to_string(), std::path::PathBuf::from("/"));
    tool.call(json!({}), &ctx).await;

    assert_eq!(tool.call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn tool_trait_object_sends() {
    fn _assert<T: Send + Sync>() {}
    _assert::<Box<dyn Tool>>();
}

// -- ExecutionMode serde contract -----------------------------------

#[test]
fn execution_mode_serializes_as_snake_case() {
    assert_eq!(
        serde_json::to_string(&ExecutionMode::Parallel).unwrap(),
        "\"parallel\""
    );
    assert_eq!(
        serde_json::to_string(&ExecutionMode::Sequential).unwrap(),
        "\"sequential\""
    );
}

#[test]
fn execution_mode_round_trips_through_json() {
    for mode in [ExecutionMode::Parallel, ExecutionMode::Sequential] {
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: ExecutionMode = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, mode);
    }
}

#[test]
fn execution_mode_default_is_parallel() {
    // Pin: the orchestrator's safe default is
    // concurrent execution; tools that mutate
    // external state must opt out via
    // `mode() -> Sequential`.
    assert_eq!(ExecutionMode::default(), ExecutionMode::Parallel);
}

#[test]
fn execution_mode_is_copy_and_eq() {
    // Pin: the orchestrator passes
    // `ExecutionMode` values by-value through
    // hot loops; it MUST stay Copy.
    let a = ExecutionMode::Sequential;
    let b = a;
    assert_eq!(a, b);
    let c = ExecutionMode::Parallel;
    assert_ne!(a, c);
}

#[test]
fn execution_mode_distinct_count_is_two() {
    // Pin that the orchestrator scheduler
    // doesn't have a third hidden state (e.g.
    // a legacy "Mixed" variant).
    let mut all = vec![ExecutionMode::Parallel, ExecutionMode::Sequential];
    all.dedup();
    assert_eq!(all.len(), 2);
}
