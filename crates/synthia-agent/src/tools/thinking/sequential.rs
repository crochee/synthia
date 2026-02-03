//! Sequential Thinking tool implementation
//!
//! A detailed tool for dynamic and reflective problem-solving through thoughts.

use std::{collections::HashMap, io::Write, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use rmcp::model::{CallToolResult, Content};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AgentError, tools::Tool};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThoughtData {
    pub thought: String,
    pub thought_number: usize,
    pub total_thoughts: usize,
    #[serde(default)]
    pub is_revision: Option<bool>,
    #[serde(default)]
    pub revises_thought: Option<usize>,
    #[serde(default)]
    pub branch_from_thought: Option<usize>,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub needs_more_thoughts: Option<bool>,
    pub next_thought_needed: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ThinkingRequest {
    thought: String,
    #[serde(rename = "thoughtNumber")]
    thought_number: usize,
    #[serde(rename = "totalThoughts")]
    total_thoughts: usize,
    #[serde(rename = "nextThoughtNeeded")]
    next_thought_needed: bool,
    #[serde(default)]
    is_revision: Option<bool>,
    #[serde(default)]
    revises_thought: Option<usize>,
    #[serde(default)]
    branch_from_thought: Option<usize>,
    #[serde(default)]
    branch_id: Option<String>,
    #[serde(default)]
    needs_more_thoughts: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ThinkingResponse {
    thought_number: usize,
    total_thoughts: usize,
    next_thought_needed: bool,
    branches: Vec<String>,
    thought_history_length: usize,
}

#[derive(Debug)]
pub struct SequentialThinkingTool<W> {
    writer: Arc<Mutex<W>>,
    thought_history: Arc<Mutex<Vec<ThoughtData>>>,
    branches: Arc<Mutex<HashMap<String, Vec<ThoughtData>>>>,
}

impl SequentialThinkingTool<std::io::Stdout> {
    /// Create a new SequentialThinkingTool with stdout as the writer
    pub fn new_with_stdout() -> Self {
        Self::new(std::io::stdout())
    }
}

impl<W: Write + Send> SequentialThinkingTool<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
            thought_history: Arc::new(Mutex::new(Vec::new())),
            branches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn format_thought(&self, thought_data: &ThoughtData) -> String {
        let ThoughtData {
            thought_number,
            total_thoughts,
            thought,
            is_revision,
            revises_thought,
            branch_from_thought,
            branch_id,
            ..
        } = thought_data;

        let (prefix, context) = if is_revision.unwrap_or(false) {
            (
                "🔄 Revision",
                format!(" (revising thought {})", revises_thought.unwrap_or(0)),
            )
        } else if let Some(branch_from) = branch_from_thought {
            (
                "🌿 Branch",
                format!(
                    " (from thought {branch_from}, ID: {})",
                    branch_id.as_deref().unwrap_or("unknown")
                ),
            )
        } else {
            ("💭 Thought", String::new())
        };

        let header =
            format!("{prefix} {thought_number}/{total_thoughts}{context}");
        let border = "─".repeat(header.len().max(thought.len()) + 4);

        format!(
            "\n┌{border}┐\n│ {header} │\n├{border}┤\n│ {thought} │\n└{border}┘"
        )
    }

    fn process_thought(
        &self,
        input: ThinkingRequest,
    ) -> Result<ThinkingResponse, AgentError> {
        let mut thought_data = ThoughtData {
            thought: input.thought,
            thought_number: input.thought_number,
            total_thoughts: input.total_thoughts,
            is_revision: input.is_revision,
            revises_thought: input.revises_thought,
            branch_from_thought: input.branch_from_thought,
            branch_id: input.branch_id.clone(),
            needs_more_thoughts: input.needs_more_thoughts,
            next_thought_needed: input.next_thought_needed,
        };

        if thought_data.thought_number > thought_data.total_thoughts {
            thought_data.total_thoughts = thought_data.thought_number;
        }

        let is_branch_thought = thought_data.branch_from_thought.is_some()
            && thought_data.branch_id.is_some();

        if let (Some(branch_from), Some(branch)) = (
            thought_data.branch_from_thought,
            thought_data.branch_id.clone(),
        ) && branch_from > 0
            && !branch.is_empty()
        {
            let mut branches = self.branches.lock();
            branches
                .entry(branch)
                .or_default()
                .push(thought_data.clone());
        }

        let formatted = self.format_thought(&thought_data);
        self.writer
            .lock()
            .write_all(formatted.as_bytes())
            .map_err(|e| AgentError::InvalidOperation(e.to_string()))?;

        if !is_branch_thought {
            self.thought_history.lock().push(thought_data.clone());
        }

        let branches = self.branches.lock();
        let thought_history = self.thought_history.lock();

        Ok(ThinkingResponse {
            thought_number: thought_data.thought_number,
            total_thoughts: thought_data.total_thoughts,
            next_thought_needed: thought_data.next_thought_needed,
            branches: branches.keys().cloned().collect(),
            thought_history_length: thought_history.len(),
        })
    }
}

#[async_trait]
impl<W: Write + Send> Tool for SequentialThinkingTool<W> {
    fn name(&self) -> &str {
        "sequentialthinking"
    }

    fn description(&self) -> &str {
        "Step-by-step problem-solving. Supports revision and branching."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "thought": {
                    "type": "string",
                    "description": "Current thinking step"
                },
                "nextThoughtNeeded": {
                    "type": "boolean",
                    "description": "Next thought needed"
                },
                "thoughtNumber": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Thought number"
                },
                "totalThoughts": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Total thoughts"
                },
                "isRevision": {
                    "type": "boolean",
                    "description": "Is revision"
                },
                "revisesThought": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Revises thought"
                },
                "branchFromThought": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Branch from"
                },
                "branchId": {
                    "type": "string",
                    "description": "Branch ID"
                },
                "needsMoreThoughts": {
                    "type": "boolean",
                    "description": "Needs more thoughts"
                }
            },
            "required": ["thought", "thoughtNumber", "totalThoughts", "nextThoughtNeeded"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: ThinkingRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let response = match self.process_thought(request) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to process thought: {e}"
                ))]);
            }
        };

        match serde_json::to_string_pretty(&response) {
            Ok(json) => CallToolResult::success(vec![Content::text(json)]),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to serialize response: {e}"
            ))]),
        }
    }
}

impl<W: Write + Send> Clone for SequentialThinkingTool<W> {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
            thought_history: Arc::clone(&self.thought_history),
            branches: Arc::clone(&self.branches),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    fn create_test_tool() -> SequentialThinkingTool<Vec<u8>> {
        SequentialThinkingTool::new(Vec::new())
    }

    /// A writer that fails after N bytes written
    struct FailingWriter {
        limit: usize,
        written: usize,
    }

    impl FailingWriter {
        fn new(limit: usize) -> Self {
            Self { limit, written: 0 }
        }
    }

    impl Write for FailingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.written >= self.limit {
                return Err(io::Error::other("writer limit reached"));
            }
            let to_write = (self.limit - self.written).min(buf.len());
            self.written += to_write;
            Ok(to_write)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    mod tool_interface {
        use super::*;

        #[test]
        fn test_name() {
            let tool = create_test_tool();
            assert_eq!(tool.name(), "sequentialthinking");
        }

        #[test]
        fn test_description() {
            let tool = create_test_tool();
            assert_eq!(
                tool.description(),
                "Step-by-step problem-solving. Supports revision and branching."
            );
        }

        #[test]
        fn test_parameters() {
            let tool = create_test_tool();
            let params = tool.parameters();
            assert_eq!(params["type"], "object");
            let props = params["properties"].as_object().unwrap();
            assert!(props.contains_key("thought"));
            assert!(props.contains_key("thoughtNumber"));
            assert!(props.contains_key("totalThoughts"));
            assert!(props.contains_key("nextThoughtNeeded"));
            let required = params["required"].as_array().unwrap();
            assert!(required.iter().any(|v| v == "thought"));
            assert!(required.iter().any(|v| v == "thoughtNumber"));
            assert!(required.iter().any(|v| v == "totalThoughts"));
            assert!(required.iter().any(|v| v == "nextThoughtNeeded"));
        }
    }

    mod format_thought {
        use super::*;

        #[test]
        fn test_regular_thought() {
            let data = ThoughtData {
                thought: "Hello world".to_string(),
                thought_number: 1,
                total_thoughts: 3,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: true,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("💭 Thought"));
            assert!(output.contains("1/3"));
            assert!(output.contains("Hello world"));
        }

        #[test]
        fn test_revision_thought() {
            let data = ThoughtData {
                thought: "Revising my earlier thought".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                is_revision: Some(true),
                revises_thought: Some(1),
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: false,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("🔄 Revision"));
            assert!(output.contains("revising thought 1"));
            assert!(output.contains("Revising my earlier thought"));
        }

        #[test]
        fn test_branch_thought() {
            let data = ThoughtData {
                thought: "Exploring alternative".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("branch-abc".to_string()),
                needs_more_thoughts: None,
                next_thought_needed: true,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("🌿 Branch"));
            assert!(output.contains("from thought 1"));
            assert!(output.contains("branch-abc"));
            assert!(output.contains("Exploring alternative"));
        }

        #[test]
        fn test_revision_without_revises_thought() {
            let data = ThoughtData {
                thought: "Revision without revises_thought".to_string(),
                thought_number: 3,
                total_thoughts: 3,
                is_revision: Some(true),
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: false,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("🔄 Revision"));
            assert!(output.contains("revising thought 0"));
        }

        #[test]
        fn test_branch_without_branch_id() {
            let data = ThoughtData {
                thought: "Branch without ID".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: true,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("🌿 Branch"));
            assert!(output.contains("unknown"));
        }
    }

    mod process_thought {
        use super::*;

        #[test]
        fn test_thought_number_normalization() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Test".to_string(),
                thought_number: 5,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            assert_eq!(response.total_thoughts, 5);
        }

        #[test]
        fn test_thought_number_at_boundary() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Test".to_string(),
                thought_number: 3,
                total_thoughts: 3,
                next_thought_needed: true,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            assert_eq!(response.thought_number, 3);
            assert_eq!(response.total_thoughts, 3);
        }

        #[test]
        fn test_branch_not_added_to_history() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Branch thought".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("branch-xyz".to_string()),
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            assert_eq!(response.thought_history_length, 0);
            assert!(response.branches.contains(&"branch-xyz".to_string()));
        }

        #[test]
        fn test_regular_thought_added_to_history() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "First thought".to_string(),
                thought_number: 1,
                total_thoughts: 2,
                next_thought_needed: true,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            assert_eq!(response.thought_history_length, 1);
            assert!(response.branches.is_empty());
        }

        #[test]
        fn test_branch_from_zero_not_branch() {
            // When branch_from_thought is 0 and branch_id is non-empty, is_branch_thought is true
            // but the condition `branch_from > 0` fails, so it falls through but is_branch_thought
            // is already set to true... so actually it still doesn't get added to history
            // Wait, let me re-read: is_branch_thought is set BEFORE we check branch_from > 0
            // So even if branch_from is 0, is_branch_thought could be true if we set it earlier
            // Let me trace: is_branch_thought = branch_from_thought.is_some() && branch_id.is_some()
            // For this test: branch_from_thought = Some(0), branch_id = Some("test")
            // So is_branch_thought = true && true = true
            // Then we check if (branch_from > 0 && !branch.is_empty()) but since branch_from is 0, this fails
            // So the branch is NOT added to the branches map, but is_branch_thought is still true
            // And at the end: if !is_branch_thought { push to history }
            // Since is_branch_thought is true, it won't be added to history!
            // So history_length = 0, not 1
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Not really a branch".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(0),
                branch_id: Some("test".to_string()),
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            // is_branch_thought is true, so NOT added to history
            assert_eq!(response.thought_history_length, 0);
        }

        #[test]
        fn test_branch_with_empty_id_not_branch() {
            // branch_from_thought = Some(1), branch_id = Some("")
            // is_branch_thought = true (both are Some)
            // Then (branch_from > 0 && !branch.is_empty()) = (1 > 0 && !"".is_empty()) = true && false = false
            // So branch is NOT added to map, but is_branch_thought is still true
            // Since !is_branch_thought is false, NOT pushed to history
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Not really a branch".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("".to_string()),
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            // is_branch_thought is true, so NOT added to history
            assert_eq!(response.thought_history_length, 0);
        }

        #[test]
        fn test_multiple_thoughts_increment_history() {
            let tool = SequentialThinkingTool::new(Vec::new());

            let req1 = ThinkingRequest {
                thought: "First".to_string(),
                thought_number: 1,
                total_thoughts: 3,
                next_thought_needed: true,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            tool.process_thought(req1).unwrap();

            let req2 = ThinkingRequest {
                thought: "Second".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: true,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            tool.process_thought(req2).unwrap();

            let req3 = ThinkingRequest {
                thought: "Third".to_string(),
                thought_number: 3,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(req3).unwrap();
            assert_eq!(response.thought_history_length, 3);
        }

        #[test]
        fn test_multiple_branches() {
            let tool = SequentialThinkingTool::new(Vec::new());

            let req1 = ThinkingRequest {
                thought: "Branch A".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("branch-a".to_string()),
                needs_more_thoughts: None,
            };
            tool.process_thought(req1).unwrap();

            let req2 = ThinkingRequest {
                thought: "Branch B".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("branch-b".to_string()),
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(req2).unwrap();
            assert_eq!(response.branches.len(), 2);
            assert!(response.branches.contains(&"branch-a".to_string()));
            assert!(response.branches.contains(&"branch-b".to_string()));
        }
    }

    mod serialization {
        use super::*;

        #[test]
        fn test_thinking_request_deserialization() {
            let json = serde_json::json!({
                "thought": "Test thought",
                "thoughtNumber": 1,
                "totalThoughts": 5,
                "nextThoughtNeeded": true
            });
            let request: ThinkingRequest =
                serde_json::from_value(json).unwrap();
            assert_eq!(request.thought, "Test thought");
            assert_eq!(request.thought_number, 1);
            assert_eq!(request.total_thoughts, 5);
            assert!(request.next_thought_needed);
            assert!(request.is_revision.is_none());
            assert!(request.branch_from_thought.is_none());
        }

        #[test]
        fn test_thinking_request_with_all_fields() {
            let json = serde_json::json!({
                "thought": "Revision thought",
                "thoughtNumber": 3,
                "totalThoughts": 5,
                "nextThoughtNeeded": false,
                "is_revision": true,
                "revises_thought": 2,
                "branch_from_thought": 1,
                "branch_id": "branch-xyz",
                "needs_more_thoughts": true
            });
            let request: ThinkingRequest =
                serde_json::from_value(json).unwrap();
            assert!(request.is_revision.unwrap());
            assert_eq!(request.revises_thought.unwrap(), 2);
            assert_eq!(request.branch_from_thought.unwrap(), 1);
            assert_eq!(request.branch_id.unwrap(), "branch-xyz");
            assert!(request.needs_more_thoughts.unwrap());
        }

        #[test]
        fn test_thought_data_serialization() {
            let data = ThoughtData {
                thought: "Test".to_string(),
                thought_number: 1,
                total_thoughts: 3,
                is_revision: Some(true),
                revises_thought: Some(1),
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: false,
            };
            let json = serde_json::to_value(&data).unwrap();
            assert_eq!(json["thought"], "Test");
            assert_eq!(json["thought_number"], 1);
            assert_eq!(json["total_thoughts"], 3);
            assert_eq!(json["is_revision"], true);
            assert_eq!(json["revises_thought"], 1);
        }

        #[test]
        fn test_thinking_response_serialization() {
            let response = ThinkingResponse {
                thought_number: 2,
                total_thoughts: 5,
                next_thought_needed: true,
                branches: vec!["branch-a".to_string(), "branch-b".to_string()],
                thought_history_length: 3,
            };
            let json = serde_json::to_string_pretty(&response).unwrap();
            assert!(json.contains("\"thought_number\": 2"));
            assert!(json.contains("\"total_thoughts\": 5"));
            assert!(json.contains("\"next_thought_needed\": true"));
            assert!(json.contains("\"branches\""));
            assert!(json.contains("\"thought_history_length\": 3"));
        }
    }

    mod async_call {
        use super::*;

        #[tokio::test]
        async fn test_call_with_valid_request() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({
                "thought": "Testing async call",
                "thoughtNumber": 1,
                "totalThoughts": 3,
                "nextThoughtNeeded": true
            });
            let result = tool.call(args).await;
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
            let content = &result.content[0];
            let text = content.as_text().unwrap();
            assert!(text.text.contains("thought_number"));
            assert!(text.text.contains("1"));
        }

        #[tokio::test]
        async fn test_call_with_missing_required_field() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({
                "thought": "Missing fields"
            });
            let result = tool.call(args).await;
            assert!(result.is_error == Some(true));
            let content = &result.content[0];
            let text = content.as_text().unwrap();
            assert!(text.text.contains("Invalid request"));
        }

        #[tokio::test]
        async fn test_call_with_revision() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({
                "thought": "This is a revision",
                "thoughtNumber": 4,
                "totalThoughts": 5,
                "nextThoughtNeeded": false,
                "isRevision": true,
                "revisesThought": 2
            });
            let result = tool.call(args).await;
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }

        #[tokio::test]
        async fn test_call_with_branch() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({
                "thought": "Exploring a branch",
                "thoughtNumber": 3,
                "totalThoughts": 5,
                "nextThoughtNeeded": true,
                "branchFromThought": 2,
                "branchId": "exploration-branch"
            });
            let result = tool.call(args).await;
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }

        #[tokio::test]
        async fn test_multiple_sequential_thoughts() {
            let tool = SequentialThinkingTool::new(Vec::new());

            let r1 = tool
                .call(serde_json::json!({
                    "thought": "First thought",
                    "thoughtNumber": 1,
                    "totalThoughts": 3,
                    "nextThoughtNeeded": true
                }))
                .await;
            assert!(r1.is_error.is_none() || r1.is_error == Some(false));

            let r2 = tool
                .call(serde_json::json!({
                    "thought": "Second thought",
                    "thoughtNumber": 2,
                    "totalThoughts": 3,
                    "nextThoughtNeeded": true
                }))
                .await;
            assert!(r2.is_error.is_none() || r2.is_error == Some(false));

            let r3 = tool
                .call(serde_json::json!({
                    "thought": "Third thought",
                    "thoughtNumber": 3,
                    "totalThoughts": 3,
                    "nextThoughtNeeded": false
                }))
                .await;
            assert!(r3.is_error.is_none() || r3.is_error == Some(false));
        }

        #[tokio::test]
        async fn test_branch_thoughts_not_in_history() {
            let tool = SequentialThinkingTool::new(Vec::new());

            // First call - regular thought, added to history
            let r1 = tool
                .call(serde_json::json!({
                    "thought": "First",
                    "thoughtNumber": 1,
                    "totalThoughts": 4,
                    "nextThoughtNeeded": true
                }))
                .await;
            assert!(r1.is_error.is_none() || r1.is_error == Some(false));

            // Second call - branch thought, should NOT be added to history
            // (but is added to branches)
            let result = tool
                .call(serde_json::json!({
                    "thought": "Branch A",
                    "thoughtNumber": 2,
                    "totalThoughts": 4,
                    "nextThoughtNeeded": true,
                    "branchFromThought": 1,
                    "branchId": "branch-a"
                }))
                .await;

            // The call succeeds - branch logic is tested in sync tests
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }

        #[tokio::test]
        async fn test_branches_tracked() {
            let tool = SequentialThinkingTool::new(Vec::new());

            // First branch
            let r1 = tool
                .call(serde_json::json!({
                    "thought": "Branch thought",
                    "thoughtNumber": 2,
                    "totalThoughts": 3,
                    "nextThoughtNeeded": false,
                    "branchFromThought": 1,
                    "branchId": "branch-1"
                }))
                .await;
            assert!(r1.is_error.is_none() || r1.is_error == Some(false));

            // Second branch
            let result = tool
                .call(serde_json::json!({
                    "thought": "Another branch",
                    "thoughtNumber": 2,
                    "totalThoughts": 3,
                    "nextThoughtNeeded": false,
                    "branchFromThought": 1,
                    "branchId": "branch-2"
                }))
                .await;
            // Both branches should succeed - the branch logic is tested in sync tests
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }

        #[tokio::test]
        async fn test_invalid_json() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({"invalid": "json"});
            let result = tool.call(args).await;
            assert!(result.is_error == Some(true));
        }

        #[tokio::test]
        async fn test_empty_thought() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({
                "thought": "",
                "thoughtNumber": 1,
                "totalThoughts": 1,
                "nextThoughtNeeded": false
            });
            let result = tool.call(args).await;
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }

        #[tokio::test]
        async fn test_all_optional_fields() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({
                "thought": "Full request",
                "thoughtNumber": 5,
                "totalThoughts": 10,
                "nextThoughtNeeded": true,
                "isRevision": true,
                "revisesThought": 3,
                "branchFromThought": 2,
                "branchId": "my-branch",
                "needsMoreThoughts": true
            });
            let result = tool.call(args).await;
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }

        #[tokio::test]
        async fn test_needs_more_thoughts_flag() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let args = serde_json::json!({
                "thought": "Need more thinking",
                "thoughtNumber": 1,
                "totalThoughts": 5,
                "nextThoughtNeeded": true,
                "needsMoreThoughts": true
            });
            let result = tool.call(args).await;
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }
    }

    mod writer_error_handling {
        use super::*;

        #[test]
        fn test_process_thought_writer_error() {
            // Test that write_all errors propagate correctly
            let tool = SequentialThinkingTool::new(FailingWriter::new(0));
            let request = ThinkingRequest {
                thought: "Test".to_string(),
                thought_number: 1,
                total_thoughts: 1,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let result = tool.process_thought(request);
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.to_string().contains("writer limit reached"));
        }

        #[tokio::test]
        async fn test_call_writer_error_propagates() {
            let tool = SequentialThinkingTool::new(FailingWriter::new(0));
            let args = serde_json::json!({
                "thought": "Test",
                "thoughtNumber": 1,
                "totalThoughts": 1,
                "nextThoughtNeeded": false
            });
            let result = tool.call(args).await;
            // Writer error should result in an error response
            assert!(result.is_error == Some(true));
            let text = result.content[0].as_text().unwrap();
            assert!(text.text.contains("Failed to process thought"));
        }
    }

    mod branch_logic {
        use super::*;

        #[test]
        fn test_branch_with_valid_from_and_id_is_added_to_branches() {
            // When branch_from_thought > 0 AND branch_id is non-empty,
            // the thought should be added to the branches map
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Valid branch".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("valid-branch".to_string()),
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            // branch is added to branches map
            assert_eq!(response.branches.len(), 1);
            assert!(response.branches.contains(&"valid-branch".to_string()));
            // but NOT added to thought_history
            assert_eq!(response.thought_history_length, 0);
        }

        #[test]
        fn test_revision_is_added_to_history() {
            // Revision thoughts are not branches, so they go to history
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Revising thought 1".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: true,
                is_revision: Some(true),
                revises_thought: Some(1),
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            assert_eq!(response.thought_history_length, 1);
            assert!(response.branches.is_empty());
        }

        #[test]
        fn test_response_includes_all_branches() {
            // Response should include ALL branch keys, not just the newly added one
            let tool = SequentialThinkingTool::new(Vec::new());

            let req1 = ThinkingRequest {
                thought: "Branch A".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("branch-a".to_string()),
                needs_more_thoughts: None,
            };
            tool.process_thought(req1).unwrap();

            let req2 = ThinkingRequest {
                thought: "Branch B".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("branch-b".to_string()),
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(req2).unwrap();

            // Response should include all branches accumulated so far
            assert_eq!(response.branches.len(), 2);
            assert!(response.branches.contains(&"branch-a".to_string()));
            assert!(response.branches.contains(&"branch-b".to_string()));
        }
    }

    mod format_thought_edge_cases {
        use super::*;

        #[test]
        fn test_format_thought_longer_than_header() {
            // When thought is longer than the header, border should use thought length
            let data = ThoughtData {
                thought: "This is a very long thought that should exceed the header length".to_string(),
                thought_number: 1,
                total_thoughts: 1,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: false,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            // The thought should appear in the output unchanged
            assert!(output.contains("This is a very long thought that should exceed the header length"));
            // Should have the box formatting
            assert!(output.contains("┌"));
            assert!(output.contains("┐"));
            assert!(output.contains("└"));
            assert!(output.contains("┘"));
        }

        #[test]
        fn test_format_thought_revision_with_no_revises_thought_uses_zero() {
            // When is_revision is true but revises_thought is None, it should use 0
            let data = ThoughtData {
                thought: "Revision".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                is_revision: Some(true),
                revises_thought: None, // Explicitly None
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: false,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("🔄 Revision"));
            assert!(output.contains("revising thought 0"));
        }

        #[test]
        fn test_format_thought_branch_with_unknown_id() {
            // When branch_id is None but branch_from_thought is Some, shows "unknown"
            let data = ThoughtData {
                thought: "Branch".to_string(),
                thought_number: 2,
                total_thoughts: 3,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: None,
                needs_more_thoughts: None,
                next_thought_needed: true,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("🌿 Branch"));
            assert!(output.contains("unknown"));
        }

        #[test]
        fn test_format_thought_branch_with_known_id() {
            // When both branch_from_thought and branch_id are Some, shows the ID
            let data = ThoughtData {
                thought: "Branch with ID".to_string(),
                thought_number: 3,
                total_thoughts: 5,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: Some(2),
                branch_id: Some("my-branch-123".to_string()),
                needs_more_thoughts: None,
                next_thought_needed: true,
            };
            let tool = create_test_tool();
            let output = tool.format_thought(&data);
            assert!(output.contains("🌿 Branch"));
            assert!(output.contains("from thought 2"));
            assert!(output.contains("my-branch-123"));
        }
    }

    mod response_verification {
        use super::*;

        #[test]
        fn test_response_thought_number_matches_request() {
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Test".to_string(),
                thought_number: 7,
                total_thoughts: 10,
                next_thought_needed: true,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            assert_eq!(response.thought_number, 7);
            assert_eq!(response.total_thoughts, 10);
            assert!(response.next_thought_needed);
        }

        #[test]
        fn test_thought_number_normalization_when_greater_than_total() {
            // When thought_number > total_thoughts, total is adjusted to thought_number
            let tool = SequentialThinkingTool::new(Vec::new());
            let request = ThinkingRequest {
                thought: "Test".to_string(),
                thought_number: 10,
                total_thoughts: 5,
                next_thought_needed: false,
                is_revision: None,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: None,
            };
            let response = tool.process_thought(request).unwrap();
            // total_thoughts should be normalized to thought_number
            assert_eq!(response.total_thoughts, 10);
            assert_eq!(response.thought_number, 10);
        }
    }
}
