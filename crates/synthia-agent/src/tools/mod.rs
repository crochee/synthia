//! Tools module
//!
//! This module provides various tool implementations for the agent.
//! Tools are the primary way agents interact with the external world.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, JsonObject};
use serde_json::Value;

pub mod ask_user;
pub mod background;
pub mod builtin;
pub mod cron;
pub mod exec;
pub mod fs;
pub mod mcp;
pub mod registry;
pub mod shared;
pub mod skill;
pub mod storage;
pub mod subagent;
pub mod task;
pub mod team;
pub mod thinking;
pub mod todo;
pub mod tom;
pub mod web;
pub mod worktree;

// Re-export all tool types for external use (e.g., synthia-cli)
pub use ask_user::{
    AskUserQuestionTool,
    Question,
    QuestionAnswer,
    QuestionOption,
    QuestionRequest,
    QuestionResponse,
    QuestionSenderImpl,
};
pub use background::{BackgroundTask, register_background_tools};
pub use builtin::register_builtin_tools;
pub use cron::{
    CronFileStore,
    CronJob,
    CronJobWrapper,
    CronRun,
    register_cron_tools,
};
pub use exec::ExecTool;
pub use mcp::{McpToolCollector, get_mcp_tools};
pub use registry::ToolRegistry;
pub use skill::SkillTool;
pub use subagent::{
    ExecutorConfig,
    SubagentExecutor,
    SubagentRequest,
    SubagentTool,
};
pub use task::{Task, TaskPriority, TaskStatus, register_task_tools};
pub use team::{
    AgentStatus,
    MessageType,
    PlanRequest,
    ShutdownRequest,
    Team,
    TeamMessage,
    TeamPatch,
    TeamStatus,
    Teammate,
    TeammateStatus,
    register_team_tools,
};
pub use worktree::register_worktree_tools;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn call(&self, args: Value) -> CallToolResult;

    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        false
    }

    fn is_mutating(&self, _args: &Value) -> bool {
        // Default: assume tools are mutating (writes acquire write pool permit).
        // Read-only tools must override is_read_only() to return true.
        false
    }

    fn is_read_only(&self, _args: &Value) -> bool {
        // Default: tools are NOT read-only. Read-only tools must opt-in by
        // overriding this to return true (they will use the read pool).
        false
    }

    fn is_dangerous(&self, _args: &Value) -> bool {
        false
    }

    fn tool_kind(&self) -> ToolKind {
        ToolKind::Function
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Function,
    Mcp,
    Internal,
}

pub fn value_to_object(value: Value) -> JsonObject {
    match value {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    // ToolKind tests

    #[test]
    fn test_tool_kind_variants() {
        assert!(matches!(ToolKind::Function, ToolKind::Function));
        assert!(matches!(ToolKind::Mcp, ToolKind::Mcp));
        assert!(matches!(ToolKind::Internal, ToolKind::Internal));
    }

    #[test]
    fn test_tool_kind_debug() {
        let kind = ToolKind::Function;
        let debug = format!("{kind:?}");
        assert!(debug.contains("Function"));

        let kind = ToolKind::Mcp;
        let debug = format!("{kind:?}");
        assert!(debug.contains("Mcp"));

        let kind = ToolKind::Internal;
        let debug = format!("{kind:?}");
        assert!(debug.contains("Internal"));
    }

    #[test]
    fn test_tool_kind_clone() {
        let kind = ToolKind::Mcp;
        let cloned = kind;
        assert_eq!(kind, cloned);

        let kind = ToolKind::Function;
        let cloned = kind;
        assert_eq!(kind, cloned);

        let kind = ToolKind::Internal;
        let cloned = kind;
        assert_eq!(kind, cloned);
    }

    #[test]
    fn test_tool_kind_eq() {
        assert_eq!(ToolKind::Function, ToolKind::Function);
        assert_eq!(ToolKind::Mcp, ToolKind::Mcp);
        assert_eq!(ToolKind::Internal, ToolKind::Internal);
        assert_ne!(ToolKind::Function, ToolKind::Mcp);
        assert_ne!(ToolKind::Function, ToolKind::Internal);
        assert_ne!(ToolKind::Mcp, ToolKind::Internal);
    }

    #[test]
    fn test_tool_kind_partial_eq() {
        assert_eq!(ToolKind::Function, ToolKind::Function);
        assert_eq!(ToolKind::Mcp, ToolKind::Mcp);
    }

    #[test]
    fn test_tool_kind_copy() {
        let kind = ToolKind::Function;
        let copied = kind;
        assert_eq!(kind, copied);
    }

    // value_to_object tests

    #[test]
    fn test_value_to_object_with_object() {
        let mut map = serde_json::Map::new();
        map.insert("key".to_string(), serde_json::json!("value"));
        let value = Value::Object(map);

        let result = value_to_object(value);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("key").unwrap().as_str().unwrap(), "value");
    }

    #[test]
    fn test_value_to_object_with_non_object() {
        let value = Value::String("hello".to_string());
        let result = value_to_object(value);
        assert!(result.is_empty());

        let value = Value::Number(42.into());
        let result = value_to_object(value);
        assert!(result.is_empty());

        let value = Value::Null;
        let result = value_to_object(value);
        assert!(result.is_empty());

        let value = Value::Array(vec![]);
        let result = value_to_object(value);
        assert!(result.is_empty());

        let value = Value::Bool(true);
        let result = value_to_object(value);
        assert!(result.is_empty());
    }

    #[test]
    fn test_value_to_object_empty_object() {
        let value = Value::Object(serde_json::Map::new());
        let result = value_to_object(value);
        assert!(result.is_empty());
    }

    #[test]
    fn test_value_to_object_nested_object() {
        let mut map = serde_json::Map::new();
        map.insert("nested".to_string(), serde_json::json!({"key": "value"}));
        let value = Value::Object(map);

        let result = value_to_object(value);
        assert_eq!(result.len(), 1);
        assert!(result.get("nested").unwrap().is_object());
    }

    #[test]
    fn test_value_to_object_multiple_keys() {
        let mut map = serde_json::Map::new();
        map.insert("key1".to_string(), serde_json::json!("value1"));
        map.insert("key2".to_string(), serde_json::json!(42));
        map.insert("key3".to_string(), serde_json::json!(true));
        let value = Value::Object(map);

        let result = value_to_object(value);
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("key1").unwrap().as_str().unwrap(), "value1");
        assert_eq!(result.get("key2").unwrap().as_i64().unwrap(), 42);
        assert!(result.get("key3").unwrap().as_bool().unwrap());
    }

    // Tool trait tests

    #[test]
    fn test_tool_trait_default_methods() {
        struct DummyTool;
        #[async_trait]
        impl Tool for DummyTool {
            fn name(&self) -> &str {
                "dummy"
            }

            fn description(&self) -> &str {
                "A dummy tool"
            }

            fn parameters(&self) -> Value {
                Value::Null
            }

            async fn call(&self, _: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }
        }

        let tool = DummyTool;
        assert!(!tool.is_concurrency_safe(&Value::Null));
        // Default is_mutating returns false; read-only tools override is_read_only
        assert!(!tool.is_mutating(&Value::Null));
        assert!(!tool.is_read_only(&Value::Null));
        assert!(!tool.is_dangerous(&Value::Null));
        assert_eq!(tool.tool_kind(), ToolKind::Function);
    }

    #[test]
    fn test_tool_trait_is_concurrency_safe_override() {
        struct ConcurrencySafeTool;
        #[async_trait]
        impl Tool for ConcurrencySafeTool {
            fn name(&self) -> &str {
                "safe"
            }

            fn description(&self) -> &str {
                "A concurrency-safe tool"
            }

            fn parameters(&self) -> Value {
                Value::Null
            }

            async fn call(&self, _: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }

            fn is_concurrency_safe(&self, _args: &Value) -> bool {
                true
            }
        }

        let tool = ConcurrencySafeTool;
        assert!(tool.is_concurrency_safe(&Value::Null));
    }

    #[test]
    fn test_tool_trait_is_read_only_override() {
        struct ReadOnlyTool;
        #[async_trait]
        impl Tool for ReadOnlyTool {
            fn name(&self) -> &str {
                "readonly"
            }

            fn description(&self) -> &str {
                "A read-only tool"
            }

            fn parameters(&self) -> Value {
                Value::Null
            }

            async fn call(&self, _: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }

            fn is_read_only(&self, _args: &Value) -> bool {
                true
            }
        }

        let tool = ReadOnlyTool;
        assert!(tool.is_read_only(&Value::Null));
    }

    #[test]
    fn test_tool_trait_is_mutating_override() {
        struct MutatingTool;
        #[async_trait]
        impl Tool for MutatingTool {
            fn name(&self) -> &str {
                "mutating"
            }

            fn description(&self) -> &str {
                "A mutating tool"
            }

            fn parameters(&self) -> Value {
                Value::Null
            }

            async fn call(&self, _: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }

            fn is_mutating(&self, _args: &Value) -> bool {
                true
            }
        }

        let tool = MutatingTool;
        assert!(tool.is_mutating(&Value::Null));
    }

    #[test]
    fn test_tool_trait_is_dangerous_override() {
        struct DangerousTool;
        #[async_trait]
        impl Tool for DangerousTool {
            fn name(&self) -> &str {
                "dangerous"
            }

            fn description(&self) -> &str {
                "A dangerous tool"
            }

            fn parameters(&self) -> Value {
                Value::Null
            }

            async fn call(&self, _: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }

            fn is_dangerous(&self, _args: &Value) -> bool {
                true
            }
        }

        let tool = DangerousTool;
        assert!(tool.is_dangerous(&Value::Null));
    }

    #[test]
    fn test_tool_trait_tool_kind_mcp() {
        struct McpTool;
        #[async_trait]
        impl Tool for McpTool {
            fn name(&self) -> &str {
                "mcp_tool"
            }

            fn description(&self) -> &str {
                "An MCP tool"
            }

            fn parameters(&self) -> Value {
                Value::Null
            }

            async fn call(&self, _: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }

            fn tool_kind(&self) -> ToolKind {
                ToolKind::Mcp
            }
        }

        let tool = McpTool;
        assert_eq!(tool.tool_kind(), ToolKind::Mcp);
    }

    #[test]
    fn test_tool_trait_tool_kind_internal() {
        struct InternalTool;
        #[async_trait]
        impl Tool for InternalTool {
            fn name(&self) -> &str {
                "internal_tool"
            }

            fn description(&self) -> &str {
                "An internal tool"
            }

            fn parameters(&self) -> Value {
                Value::Null
            }

            async fn call(&self, _: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }

            fn tool_kind(&self) -> ToolKind {
                ToolKind::Internal
            }
        }

        let tool = InternalTool;
        assert_eq!(tool.tool_kind(), ToolKind::Internal);
    }

    // Test that Tool can be used in a trait object (Send + Sync bounds)
    #[test]
    fn test_tool_trait_object() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Box<dyn Tool>>();
    }
}
