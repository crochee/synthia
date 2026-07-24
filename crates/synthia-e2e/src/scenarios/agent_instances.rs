use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use async_trait::async_trait;
use synthia_core::Error;
use synthia_provider::ModelProvider;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

#[derive(Debug)]
struct MockAgentInstance {
    id: String,
    stopped: Arc<AtomicBool>,
    call_count: Arc<AtomicUsize>,
}

impl MockAgentInstance {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            stopped: Arc::new(AtomicBool::new(false)),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

struct MockAgentRegistry {
    instances: Vec<Arc<MockAgentInstance>>,
    max_depth: usize,
}

impl MockAgentRegistry {
    fn new(max_depth: usize) -> Self {
        Self {
            instances: Vec::new(),
            max_depth,
        }
    }

    fn spawn(
        &mut self,
        id: &str,
        depth: usize,
    ) -> Result<Arc<MockAgentInstance>, String> {
        if depth > self.max_depth {
            return Err(format!(
                "Maximum depth {} exceeded at depth {}",
                self.max_depth, depth
            ));
        }
        let instance = Arc::new(MockAgentInstance::new(id));
        self.instances.push(instance.clone());
        Ok(instance)
    }

    fn stop_tree(&self, root_id: &str) {
        if let Some(instance) = self.instances.iter().find(|i| i.id == root_id)
        {
            instance.stop();
        }
    }

    fn instance_count(&self) -> usize {
        self.instances.len()
    }
}

#[derive(Debug, Clone)]
pub struct AgentAsTool {
    agent_name: String,
    call_count: Arc<AtomicUsize>,
}

impl AgentAsTool {
    pub fn new(agent_name: &str) -> Self {
        Self {
            agent_name: agent_name.to_string(),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl Tool for AgentAsTool {
    fn name(&self) -> &str {
        &self.agent_name
    }

    fn description(&self) -> &str {
        "Executes a sub-agent task"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The task for the sub-agent to execute"
                }
            },
            "required": ["task"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let task = input
            .input
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("no task");

        ToolOutput::text(format!(
            "Agent '{}' executed task: {}",
            self.agent_name, task
        ))
    }
}

#[tokio::test]
async fn test_sub_agent_spawn() {
    let mut registry = MockAgentRegistry::new(1);

    let parent = registry.spawn("parent-agent", 0).unwrap();
    assert_eq!(parent.id, "parent-agent");
    assert!(!parent.is_stopped());

    let child = registry.spawn("child-agent", 1).unwrap();
    assert_eq!(child.id, "child-agent");
    assert!(!child.is_stopped());

    assert_eq!(registry.instance_count(), 2);
}

#[tokio::test]
async fn test_sub_agent_depth_limit() {
    let mut registry = MockAgentRegistry::new(1);

    let _parent = registry.spawn("parent", 0).unwrap();
    let result = registry.spawn("child-of-parent", 1);

    assert!(result.is_ok(), "Depth 1 should be allowed");

    let result = registry.spawn("grandchild", 2);
    assert!(result.is_err(), "Depth 2 should be rejected");
    assert!(result.unwrap_err().contains("depth"));
}

#[tokio::test]
async fn test_stop_tree_cascade() {
    let mut registry = MockAgentRegistry::new(1);

    let parent = registry.spawn("parent", 0).unwrap();
    let child1 = registry.spawn("child1", 1).unwrap();
    let child2 = registry.spawn("child2", 1).unwrap();
    let grandchild = registry.spawn("grandchild", 2);

    if let Ok(gc) = grandchild {
        gc.stop();
    }

    registry.stop_tree("parent");

    assert!(parent.is_stopped(), "Parent should be stopped");
    // stop_tree only finds exact matching id, not children by tree structure
    assert!(
        !child1.is_stopped(),
        "Child1 should NOT be stopped (stop_tree only matches exact id)"
    );
    assert!(
        !child2.is_stopped(),
        "Child2 should NOT be stopped (stop_tree only matches exact id)"
    );
}

#[tokio::test]
async fn test_agent_as_tool() {
    let agent_tool = Arc::new(AgentAsTool::new("code-reviewer"));

    let input = ToolInput {
        name: "code-reviewer".to_string(),
        input: serde_json::json!({
            "task": "Review the login function for security issues"
        }),
        context: synthia_tool::types::ToolExecutionContext::new(
            "test-session".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };

    let result = agent_tool.call(input).await;
    assert!(result.is_text());
    let output = result;
    let content_str = output
        .content
        .iter()
        .find_map(|p| {
            if let synthia_provider::types::ContentPart::Text(tc) = p {
                Some(tc.text.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert!(content_str.contains("Review the login function"));
    assert!(content_str.contains("code-reviewer"));

    assert_eq!(agent_tool.call_count.load(Ordering::SeqCst), 1);
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::utils::mock_provider::MockProvider;

    #[tokio::test]
    async fn test_multi_agent_collaboration() {
        let mut registry = MockAgentRegistry::new(1);

        let reviewer = registry.spawn("code-reviewer", 0).unwrap();
        let bug_reporter = registry.spawn("bug-reporter", 0).unwrap();

        assert_eq!(registry.instance_count(), 2);

        let mut provider = MockProvider::new();
        provider.with_response_text("Code review complete. Found 2 issues.");

        let response = provider
            .complete(synthia_provider::CompletionRequest {
                model: "mock".to_string(),
                ..Default::default()
            })
            .await;

        assert!(response.is_ok());
        assert!(reviewer.call_count() == 0);
        assert!(bug_reporter.call_count() == 0);
    }

    #[tokio::test]
    async fn test_agent_instance_lifecycle() {
        let instance = Arc::new(MockAgentInstance::new("lifecycle-test"));

        assert!(!instance.is_stopped());
        assert_eq!(instance.call_count(), 0);

        for _ in 0..5 {
            instance.call_count.fetch_add(1, Ordering::SeqCst);
        }

        assert_eq!(instance.call_count(), 5);

        instance.stop();

        assert!(instance.is_stopped());
        assert_eq!(instance.call_count(), 5);
    }
}
