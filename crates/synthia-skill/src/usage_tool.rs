//! LLM-callable `query_skill_usage` tool — exposes the
//! [`SkillUsageTracker`] statistics through the standard `Tool` trait
//! so the LLM (and the user) can query skill usage history at any
//! point in a session.
//!
//! Read-only: no permission required, no side effects, safe to run
//! concurrently with other read-only tools.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_tool::{Tool, ToolInput, ToolOutput};

use crate::usage::SkillUsageTracker;

/// Tool name exposed to the LLM.
pub const QUERY_SKILL_USAGE_TOOL_NAME: &str = "query_skill_usage";

/// Tool that returns the [`SkillUsageTracker`] statistics as JSON.
///
/// Arguments:
/// - `name` (optional, string): filter to a single skill. If omitted,
///   the tool returns stats for every recorded skill.
pub struct QuerySkillUsageTool {
    tracker: Arc<SkillUsageTracker>,
}

impl QuerySkillUsageTool {
    pub fn new(tracker: Arc<SkillUsageTracker>) -> Self {
        Self { tracker }
    }

    pub fn tracker(&self) -> &Arc<SkillUsageTracker> {
        &self.tracker
    }
}

#[async_trait]
impl Tool for QuerySkillUsageTool {
    fn name(&self) -> &str {
        QUERY_SKILL_USAGE_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Return skill-usage statistics recorded so far in this \
         session. Optionally filter to a single skill by passing the \
         'name' parameter. Read-only; safe to call any time."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Optional. If provided, return stats for this skill only."
                }
            },
            "required": []
        })
    }

    /// Read-only access to an in-memory tracker. Safe to run
    /// concurrently with any other tool.
    fn is_concurrency_safe(&self) -> bool {
        true
    }

    /// LLM and user can both invoke it. The LLM uses it to see which
    /// skills it has been activating; the user uses it to inspect the
    /// session's skill footprint.
    fn is_user_invocable(&self) -> bool {
        true
    }

    /// Pure read. Stay parallel.
    fn execution_mode(&self) -> synthia_tool::traits::ExecutionMode {
        synthia_tool::traits::ExecutionMode::Parallel
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        match input.input.get("name").and_then(|v| v.as_str()) {
            Some(name) => match self.tracker.get_stats(name) {
                Some(record) => match serde_json::to_string_pretty(&record) {
                    Ok(text) => ToolOutput::text(text),
                    Err(e) => ToolOutput::error(format!(
                        "Failed to serialise stats: {}",
                        e
                    )),
                },
                None => ToolOutput::error(format!(
                    "No usage stats recorded for skill '{}'",
                    name
                )),
            },
            None => {
                let all = self.tracker.get_all_stats();
                match serde_json::to_string_pretty(&all) {
                    Ok(text) => ToolOutput::text(text),
                    Err(e) => ToolOutput::error(format!(
                        "Failed to serialise stats: {}",
                        e
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use synthia_tool::types::ToolExecutionContext;

    use super::*;

    fn make_input(value: serde_json::Value) -> ToolInput {
        ToolInput {
            name: QUERY_SKILL_USAGE_TOOL_NAME.to_string(),
            input: value,
            context: ToolExecutionContext::new(
                "session-1".to_string(),
                PathBuf::from("/tmp"),
            ),
        }
    }

    #[test]
    fn tool_exposes_expected_name_and_optional_name_param() {
        let tracker = Arc::new(SkillUsageTracker::new());
        let tool = QuerySkillUsageTool::new(tracker);
        assert_eq!(tool.name(), QUERY_SKILL_USAGE_TOOL_NAME);
        let schema = tool.parameters();
        let properties = schema.get("properties").unwrap().as_object().unwrap();
        assert!(properties.contains_key("name"));
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.is_empty());
        assert!(tool.is_user_invocable());
        assert!(tool.is_concurrency_safe());
    }

    #[tokio::test]
    async fn tool_returns_all_stats_when_no_filter() {
        let tracker = Arc::new(SkillUsageTracker::new());
        tracker.record_match("skill_a", 100);
        tracker.record_activation("skill_a", 200);
        let tool = QuerySkillUsageTool::new(tracker);
        let output = tool.call(make_input(serde_json::json!({}))).await;
        assert!(output.is_error.is_none() || output.is_error == Some(false));
        let text: String =
            output.content.iter().filter_map(|p| p.text()).collect();
        assert!(text.contains("skill_a"));
    }

    #[tokio::test]
    async fn tool_filters_to_specific_skill() {
        let tracker = Arc::new(SkillUsageTracker::new());
        tracker.record_match("skill_a", 100);
        tracker.record_match("skill_b", 50);
        let tool = QuerySkillUsageTool::new(tracker);
        let output = tool
            .call(make_input(serde_json::json!({"name": "skill_a"})))
            .await;
        let text: String =
            output.content.iter().filter_map(|p| p.text()).collect();
        assert!(text.contains("skill_a"));
        // The single-record payload should not include the other skill
        assert!(!text.contains("skill_b"));
    }

    #[tokio::test]
    async fn tool_errors_when_filtered_skill_unknown() {
        let tracker = Arc::new(SkillUsageTracker::new());
        let tool = QuerySkillUsageTool::new(tracker);
        let output = tool
            .call(make_input(serde_json::json!({"name": "missing"})))
            .await;
        assert_eq!(output.is_error, Some(true));
    }
}
