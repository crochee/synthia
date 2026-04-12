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
pub mod search;
pub mod send_user_message;
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
pub use mcp::{
    ListMcpResourcesTool,
    McpAuthTool,
    McpToolCollector,
    ReadMcpResourceTool,
    RemoteTriggerTool,
    get_mcp_tools,
};
pub use registry::ToolRegistry;
pub use search::ToolSearchTool;
pub use send_user_message::SendUserMessageTool;
pub use skill::SkillTool;
pub use subagent::{
    Agent,
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

// =============================================================================
// Mode-Aware Tool Registration
// =============================================================================

/// Common message tools used by both Team Lead and Team Member
fn create_common_message_tools(
    agent_name: Option<String>,
) -> Vec<std::sync::Arc<dyn Tool>> {
    vec![
        team::create_send_message_tool(agent_name.clone()),
        team::create_read_inbox_tool(agent_name),
    ]
}

/// Common task query tools used by both Team Lead and Team Member
fn create_common_task_query_tools() -> Vec<std::sync::Arc<dyn Tool>> {
    vec![
        task::create_task_get_tool(),
        task::create_task_list_tool(),
        task::create_task_update_tool(),
    ]
}

/// Register tools for Solo mode.
pub async fn register_solo_tools(
    registry: &ToolRegistry,
    subagent_executor: std::sync::Arc<SubagentExecutor>,
) {
    let subagent_tool = SubagentTool::new(subagent_executor);
    registry.register(std::sync::Arc::new(subagent_tool)).await;
}

/// Register tools for Team Lead mode.
pub async fn register_team_lead_tools(registry: &ToolRegistry) {
    let mut tools: Vec<std::sync::Arc<dyn Tool>> = vec![
        // Teammate management
        team::create_spawn_teammate_tool(),
        team::create_list_teammates_tool(),
        // Broadcast (lead only)
        team::create_broadcast_tool(),
        // Protocol tools
        team::create_shutdown_request_tool(),
        team::create_shutdown_response_tool(),
        team::create_plan_approval_tool(),
        // Idle tool
        team::create_idle_tool(),
        // Team management tools
        team::create_team_create_tool(),
        team::create_team_list_tool(),
        team::create_team_assign_tool(),
        team::create_team_status_tool(),
        team::create_team_update_tool(),
        team::create_team_delete_tool(),
        // Task creation tools (lead only)
        task::create_task_create_tool(),
        task::create_task_delete_tool(),
        task::create_task_stop_tool(),
        task::create_task_delegate_tool(),
    ];
    // Common tools
    tools.extend(create_common_message_tools(None));
    tools.extend(create_common_task_query_tools());
    registry.registers(tools.into_iter()).await;
}

/// Register tools for Team Member mode.
pub async fn register_team_member_tools(
    registry: &ToolRegistry,
    agent_name: String,
) {
    let mut tools: Vec<std::sync::Arc<dyn Tool>> = vec![
        // Idle tool
        team::create_idle_tool(),
        // Task claim (member only)
        task::create_claim_task_tool(),
    ];
    // Common tools
    tools.extend(create_common_message_tools(Some(agent_name)));
    tools.extend(create_common_task_query_tools());
    registry.registers(tools.into_iter()).await;
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

    // =============================================================================
    // Mode-Aware Tool Registration Tests
    // =============================================================================

    #[tokio::test]
    async fn test_register_team_lead_tools() {
        let registry = ToolRegistry::new();
        register_team_lead_tools(&registry).await;

        // Verify expected tools are present
        assert!(registry.contains("spawn_teammate"));
        assert!(registry.contains("list_teammates"));
        assert!(registry.contains("send_to_teammate"));
        assert!(registry.contains("read_inbox"));
        assert!(registry.contains("broadcast"));
        assert!(registry.contains("shutdown_request"));
        assert!(registry.contains("shutdown_response"));
        assert!(registry.contains("plan_approval"));
        assert!(registry.contains("idle"));
        assert!(registry.contains("team_create"));
        assert!(registry.contains("team_list"));
        assert!(registry.contains("team_assign"));
        assert!(registry.contains("team_status"));
        assert!(registry.contains("team_update"));
        assert!(registry.contains("team_delete"));
        assert!(registry.contains("task_create"));
        assert!(registry.contains("task_get"));
        assert!(registry.contains("task_list"));
        assert!(registry.contains("task_update"));
        assert!(registry.contains("task_delete"));
        assert!(registry.contains("task_stop"));
        assert!(registry.contains("task_delegate"));

        // Verify claim_task is NOT present (reserved for members)
        assert!(!registry.contains("claim_task"));
    }

    #[tokio::test]
    async fn test_register_team_member_tools() {
        let registry = ToolRegistry::new();
        register_team_member_tools(&registry, "test-member".to_string()).await;

        // Verify expected tools are present
        assert!(registry.contains("send_to_teammate"));
        assert!(registry.contains("read_inbox"));
        assert!(registry.contains("idle"));
        assert!(registry.contains("claim_task"));
        assert!(registry.contains("task_get"));
        assert!(registry.contains("task_list"));
        assert!(registry.contains("task_update"));

        // Verify lead-only tools are NOT present
        assert!(!registry.contains("spawn_teammate"));
        assert!(!registry.contains("broadcast"));
        assert!(!registry.contains("task_create"));
        assert!(!registry.contains("team_create"));
        assert!(!registry.contains("shutdown_request"));
        assert!(!registry.contains("plan_approval"));
    }

    #[tokio::test]
    async fn test_register_solo_tools() {
        // Create a minimal SubagentExecutor for testing
        use std::sync::Arc;

        use crate::{
            agent::Guards,
            context::DefaultContextManager,
            hooks::HookRegistry,
            model_router::{FirstModelRouter, ModelRouter},
            session::SessionFileStore,
            tools::subagent::{ExecutorConfig, SubagentExecutor},
        };

        let registry = Arc::new(ToolRegistry::new());
        let model_router: Arc<dyn ModelRouter> =
            Arc::new(FirstModelRouter::default());
        let context_manager =
            Arc::new(DefaultContextManager::new(Arc::clone(&model_router)));
        let session_manager = Arc::new(SessionFileStore::new());
        let hook_registry = Arc::new(HookRegistry::new());
        let skill_tool =
            Arc::new(SkillTool::new(std::path::PathBuf::from(".")));
        // Use a closure as event handler
        let event_handler = Arc::new(
            |_agent_name: &crate::config::AgentName,
             _event: &crate::types::AgentEvent| {},
        )
            as Arc<dyn crate::event_handler::AgentEventHandler>;
        let guards = Arc::new(Guards::new(Some(5)));

        let executor = SubagentExecutor::new(ExecutorConfig {
            tool_registry: Arc::clone(&registry),
            context_manager,
            session_manager,
            model_router,
            hook_registry,
            skill_tool,
            event_handler,
            guards,
        });

        register_solo_tools(&registry, Arc::new(executor)).await;

        // Verify subagent tool is present
        assert!(registry.contains("Agent"));

        // Verify team tools are NOT present
        assert!(!registry.contains("spawn_teammate"));
        assert!(!registry.contains("broadcast"));
        assert!(!registry.contains("claim_task"));
        assert!(!registry.contains("task_create"));
    }

    #[tokio::test]
    async fn test_tool_count_by_mode() {
        // Team Lead should have the most tools
        let lead_registry = ToolRegistry::new();
        register_team_lead_tools(&lead_registry).await;
        let lead_count = lead_registry.tool_count().await;

        // Team Member should have fewer tools
        let member_registry = ToolRegistry::new();
        register_team_member_tools(&member_registry, "test-member".to_string())
            .await;
        let member_count = member_registry.tool_count().await;

        // Verify lead has more tools than member
        assert!(lead_count > member_count);

        // Verify expected counts
        assert_eq!(lead_count, 22); // 15 team tools + 7 task tools
        assert_eq!(member_count, 7); // 3 message/idle tools + 4 task tools
    }
}
