//! Cross-module tests for the agent-tools surface.
//!
//! These tests touch several of the smaller modules at once
//! (`bus`, `coordinator`, `team`, `lifecycle_tools`) — keeping them
//! in a single file avoids the cost of re-declaring the same shared
//! helper (`test_tool_input`, `get_output_text`) in every sibling.

use std::sync::Arc;

use synthia_provider::types::ContentPart;
use synthia_tool::{
    traits::Tool,
    types::{ToolExecutionContext, ToolInput, ToolOutput},
};
use tokio_util::sync::CancellationToken;

use super::{
    agent_tool::AgentTool,
    bus::{AgentMessage, InMemoryMessageBus, MessageBus},
    coordinator::{AgentCoordinator, AgentInstance},
    lifecycle_tools::{AgentStatusTool, HandoffTool, RegisterAgentTool},
    team::SubagentManager,
};
use crate::task::types::TaskStatus;

fn test_tool_input(input: serde_json::Value) -> ToolInput {
    ToolInput {
        name: "test".to_string(),
        input,
        context: ToolExecutionContext::new(
            "test-session".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    }
}

fn get_output_text(output: &ToolOutput) -> String {
    match output.content.first() {
        Some(ContentPart::Text(t)) => t.text.clone(),
        _ => String::new(),
    }
}

#[tokio::test]
async fn test_message_bus_send_receive() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let _ = bus.register_agent("agent1");
    let _ = bus.register_agent("agent2");

    let msg = AgentMessage::new(
        "agent1".to_string(),
        "agent2".to_string(),
        "hello".to_string(),
    );

    bus.send(msg).await.unwrap();
    let received = bus.receive("agent2").await.unwrap();
    assert!(received.is_some());
    assert_eq!(received.unwrap().from, "agent1");
}

#[tokio::test]
async fn test_agent_coordinator_register() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    let agent = AgentInstance::new(
        "agent1".to_string(),
        "planner".to_string(),
        vec!["planning".to_string()],
        "You are a planner".to_string(),
        vec![],
        std::collections::HashMap::new(),
    );

    coordinator.register_agent(agent).unwrap();
    let retrieved = coordinator.get_agent("agent1");
    assert!(retrieved.is_ok());
    assert_eq!(retrieved.unwrap().role, "planner");
}

#[tokio::test]
async fn test_assign_task_capability_matching() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    let planner = AgentInstance::new(
        "planner".to_string(),
        "planner".to_string(),
        vec!["planning".to_string()],
        "You plan".to_string(),
        vec![],
        std::collections::HashMap::new(),
    );
    let executor = AgentInstance::new(
        "executor".to_string(),
        "executor".to_string(),
        vec!["coding".to_string()],
        "You code".to_string(),
        vec![],
        std::collections::HashMap::new(),
    );
    coordinator.register_agent(planner).unwrap();
    coordinator.register_agent(executor).unwrap();

    let assigned = coordinator.assign_task("plan-task", "planning").unwrap();
    assert_eq!(assigned, "planner");

    let assigned2 = coordinator.assign_task("code-task", "coding").unwrap();
    assert_eq!(assigned2, "executor");
}

#[tokio::test]
async fn test_task_dependency_tracking() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    coordinator.add_dependency("task-b".to_string(), "task-a".to_string());
    assert!(!coordinator.can_schedule("task-b"));

    let agent = AgentInstance::new(
        "agent1".to_string(),
        "worker".to_string(),
        vec!["general".to_string()],
        "You work".to_string(),
        vec![],
        std::collections::HashMap::new(),
    );
    coordinator.register_agent(agent).unwrap();

    let result_a = crate::task::types::TaskResult {
        output: "done".to_string(),
        status: TaskStatus::Success,
        exit_code: Some(0),
        artifacts: Vec::new(),
    };
    coordinator.store_result("task-a".to_string(), result_a);
    assert!(coordinator.can_schedule("task-b"));

    let ready = coordinator.get_ready_tasks();
    assert!(ready.contains(&"task-b".to_string()));
}

#[tokio::test]
async fn test_collect_structured_results() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    let result1 = crate::task::types::TaskResult {
        output: "main.rs".to_string(),
        status: TaskStatus::Success,
        exit_code: Some(0),
        artifacts: Vec::new(),
    };
    let result2 = crate::task::types::TaskResult {
        output: "lib.rs".to_string(),
        status: TaskStatus::Success,
        exit_code: Some(0),
        artifacts: Vec::new(),
    };
    coordinator.store_result("t1".to_string(), result1);
    coordinator.store_result("t2".to_string(), result2);

    let collected =
        coordinator.collect_results(&["t1".to_string(), "t2".to_string()]);
    assert_eq!(collected.len(), 2);

    let aggregated =
        coordinator.aggregate_outputs(&["t1".to_string(), "t2".to_string()]);
    assert_eq!(aggregated.len(), 2);
}

#[tokio::test]
async fn test_register_agent_tool() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));
    let tool = RegisterAgentTool::new(coordinator.clone());

    let input = test_tool_input(serde_json::json!({
        "agent_id": "test-agent",
        "role": "planner",
        "capabilities": ["planning"],
        "system_prompt": "You are a test planner",
        "tools": ["handoff"]
    }));

    let output = tool.call(input).await;
    assert!(output.is_text());
    assert!(coordinator.get_agent("test-agent").is_ok());
}

#[tokio::test]
async fn test_register_agent_tool_duplicate() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));
    let tool = RegisterAgentTool::new(coordinator.clone());

    // Register first agent
    let input1 = test_tool_input(serde_json::json!({
        "agent_id": "duplicate-test",
        "role": "worker"
    }));
    let output1 = tool.call(input1).await;
    assert!(output1.is_text());

    // Try to register duplicate
    let input2 = test_tool_input(serde_json::json!({
        "agent_id": "duplicate-test",
        "role": "planner"
    }));
    let output2 = tool.call(input2).await;
    assert!(!output2.is_text());
    assert!(get_output_text(&output2).contains("already registered"));
}

#[tokio::test]
async fn test_register_agent_tool_rejects_builtin_types() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));
    let tool = RegisterAgentTool::new(coordinator.clone());

    for builtin in ["general", "explore"] {
        let input = test_tool_input(serde_json::json!({
            "agent_id": builtin,
            "role": "worker"
        }));
        let output = tool.call(input).await;
        assert!(
            !output.is_text(),
            "registering built-in type {} should fail",
            builtin
        );
        let text = get_output_text(&output);
        assert!(
            text.contains("reserved"),
            "expected reserved error, got: {}",
            text
        );
    }
}

#[tokio::test]
async fn test_agent_status_tool_single_agent() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));
    coordinator
        .register_agent(AgentInstance::new(
            "status-test".to_string(),
            "worker".to_string(),
            vec!["test".to_string()],
            "Test agent".to_string(),
            vec![],
            std::collections::HashMap::new(),
        ))
        .unwrap();

    let tool = AgentStatusTool::new(coordinator.clone());
    let input = test_tool_input(serde_json::json!({"agent_id": "status-test"}));
    let output = tool.call(input).await;
    assert!(output.is_text());
    assert!(get_output_text(&output).contains("status-test"));
    assert!(get_output_text(&output).contains("worker"));
}

#[tokio::test]
async fn test_agent_status_tool_list_all() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    coordinator
        .register_agent(AgentInstance::new(
            "agent1".to_string(),
            "planner".to_string(),
            vec!["planning".to_string()],
            "Planner".to_string(),
            vec![],
            std::collections::HashMap::new(),
        ))
        .unwrap();
    coordinator
        .register_agent(AgentInstance::new(
            "agent2".to_string(),
            "executor".to_string(),
            vec!["coding".to_string()],
            "Executor".to_string(),
            vec![],
            std::collections::HashMap::new(),
        ))
        .unwrap();

    let tool = AgentStatusTool::new(coordinator.clone());
    let input = test_tool_input(serde_json::json!({}));
    let output = tool.call(input).await;
    assert!(output.is_text());
    assert!(get_output_text(&output).contains("agent1"));
    assert!(get_output_text(&output).contains("agent2"));
    assert!(get_output_text(&output).contains("Total registered agents: 2"));
}

#[tokio::test]
async fn test_agent_status_tool_not_found() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    let tool = AgentStatusTool::new(coordinator.clone());
    let input = test_tool_input(serde_json::json!({"agent_id": "nonexistent"}));
    let output = tool.call(input).await;
    assert!(!output.is_text());
    assert!(get_output_text(&output).contains("not found"));
}

#[tokio::test]
async fn test_handoff_tool() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let _ = bus.register_agent("sender");
    let _ = bus.register_agent("receiver");

    let tool = HandoffTool::new(bus.clone(), "sender".to_string());

    let input = test_tool_input(serde_json::json!({
        "target_agent_id": "receiver",
        "content": {"task": "test", "priority": "high"}
    }));

    let output = tool.call(input).await;
    assert!(output.is_text());
    assert!(get_output_text(&output).contains("successfully"));

    let received = bus.receive("receiver").await.unwrap();
    assert!(received.is_some());
    assert_eq!(received.unwrap().from, "sender");
}

#[tokio::test]
async fn test_handoff_tool_missing_target() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let _ = bus.register_agent("sender");

    let tool = HandoffTool::new(bus.clone(), "sender".to_string());

    let input = test_tool_input(serde_json::json!({
        "target_agent_id": "nonexistent",
        "content": {"task": "test"}
    }));

    let output = tool.call(input).await;
    assert!(!output.is_text());
    assert!(get_output_text(&output).contains("not found"));
}

#[tokio::test]
async fn test_handoff_tool_missing_content() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let _ = bus.register_agent("sender");
    let _ = bus.register_agent("receiver");

    let tool = HandoffTool::new(bus.clone(), "sender".to_string());

    let input = test_tool_input(serde_json::json!({
        "target_agent_id": "receiver"
    }));

    let output = tool.call(input).await;
    assert!(!output.is_text());
}

#[tokio::test]
async fn test_in_memory_message_bus_double_register() {
    let bus = Arc::new(InMemoryMessageBus::new());

    // Register agent first time
    let result1 = bus.register_agent("agent1");
    assert!(result1.is_ok());

    // Register same agent again should be idempotent
    let result2 = bus.register_agent("agent1");
    assert!(result2.is_ok());
}

#[tokio::test]
async fn test_message_bus_send_to_nonexistent_agent() {
    let bus = Arc::new(InMemoryMessageBus::new());

    let msg = AgentMessage::new(
        "sender".to_string(),
        "nonexistent".to_string(),
        "test".to_string(),
    );

    let result = bus.send(msg).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_agent_instance_has_capability() {
    let agent = AgentInstance::new(
        "test-agent".to_string(),
        "worker".to_string(),
        vec!["planning".to_string(), "coding".to_string()],
        "Test agent".to_string(),
        vec![],
        std::collections::HashMap::new(),
    );

    assert!(agent.has_capability("planning"));
    assert!(agent.has_capability("PLANNING")); // Case insensitive
    assert!(agent.has_capability("coding"));
    assert!(!agent.has_capability("reviewing"));
}

#[tokio::test]
async fn test_coordinator_already_registered_error() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    let agent = AgentInstance::new(
        "duplicate-agent".to_string(),
        "worker".to_string(),
        vec!["test".to_string()],
        "Test".to_string(),
        vec![],
        std::collections::HashMap::new(),
    );

    // First registration should succeed
    let result1 = coordinator.register_agent(agent.clone());
    assert!(result1.is_ok());

    // Second registration should fail
    let result2 = coordinator.register_agent(agent);
    assert!(result2.is_err());
    assert!(
        result2
            .unwrap_err()
            .to_string()
            .contains("already registered")
    );
}

#[tokio::test]
async fn test_coordinator_get_nonexistent_agent() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let coordinator = Arc::new(AgentCoordinator::new(bus));

    let result = coordinator.get_agent("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_subagent_manager_create_and_send() {
    let manager = SubagentManager::new();

    // Create an agent
    let agent_id = manager.create_agent("Test task");
    assert!(!agent_id.is_empty());

    // Send message should succeed for existing agent
    assert!(manager.send_message(&agent_id, "Hello"));
}

#[tokio::test]
async fn test_subagent_manager_teams() {
    let manager = SubagentManager::new();

    // Create a team
    let team = manager.create_team(
        "test-team",
        vec!["member1".to_string(), "member2".to_string()],
    );
    assert_eq!(team.name, "test-team");
    assert_eq!(team.members.len(), 2);

    // Delete the team
    assert!(manager.delete_team(&team.id));

    // Deleting again should return false
    assert!(!manager.delete_team(&team.id));
}

#[tokio::test]
async fn test_agent_tool_smoke() {
    // Sanity-check that the `task` tool is wired to the manager and
    // produces an error when parent_config is not set.
    let manager = Arc::new(SubagentManager::new());
    let tool = AgentTool::new(manager, false);

    let input = test_tool_input(serde_json::json!({
        "description": "do something",
        "prompt": "do the thing",
        "subagent_type": "general"
    }));
    let output = tool.call(input).await;
    assert!(!output.is_text());
    let text = get_output_text(&output);
    assert!(text.contains("parent config"));
}

#[test]
fn test_agent_tool_name_and_description() {
    let manager = Arc::new(SubagentManager::new());
    let tool = AgentTool::new(manager, false);

    assert_eq!(tool.name(), "task");
    let description = tool.description();
    assert!(description.contains("general:"));
    assert!(description.contains("explore:"));
}

#[tokio::test]
async fn test_agent_tool_description_includes_registered_agents() {
    let manager = Arc::new(SubagentManager::new());
    let coordinator = manager.get_coordinator();
    let tool = RegisterAgentTool::new(coordinator.clone());

    let input = test_tool_input(serde_json::json!({
        "agent_id": "custom-scout",
        "role": "scout",
        "capabilities": ["explore"]
    }));
    let output = tool.call(input).await;
    assert!(output.is_text(), "custom agent registration should succeed");

    let task_tool = AgentTool::new(manager, false);
    assert!(task_tool.description().contains("custom-scout"));
}

#[test]
fn test_slot_guard_drop_releases_slot() {
    // Dropping a SlotGuard without calling commit() MUST release the
    // concurrency slot back to the manager.
    let manager = Arc::new(SubagentManager::new());
    let max = manager.max_concurrent();

    // Fill all available slots.
    let mut guards: Vec<_> = (0..max)
        .map(|_| {
            manager
                .try_acquire_slot()
                .expect("acquire should succeed within limit")
        })
        .collect();
    assert!(
        manager.try_acquire_slot().is_none(),
        "quota should be exhausted after filling all slots"
    );

    // Drop one guard — its slot should be released.
    guards.pop();
    assert!(
        manager.try_acquire_slot().is_some(),
        "dropping a guard must release its slot"
    );
}

#[test]
fn test_slot_guard_commit_prevents_release() {
    // Calling commit() on a SlotGuard MUST keep the slot consumed
    // (Drop must NOT call release_slot() again).
    let manager = Arc::new(SubagentManager::new());
    let max = manager.max_concurrent();

    // Fill all but one slot.
    let _guards: Vec<_> = (0..(max - 1))
        .map(|_| {
            manager
                .try_acquire_slot()
                .expect("acquire should succeed within limit")
        })
        .collect();

    // Acquire the last slot.
    let guard = manager
        .try_acquire_slot()
        .expect("last slot should be acquirable");
    assert!(
        manager.try_acquire_slot().is_none(),
        "quota should be full after acquiring last slot"
    );

    // Commit the guard — slot must remain held (not released by Drop).
    guard.commit();
    assert!(
        manager.try_acquire_slot().is_none(),
        "commit() must NOT release the slot"
    );
}

#[test]
fn test_try_acquire_slot_returns_none_when_exhausted() {
    // When active_count >= max_concurrent, try_acquire_slot() MUST
    // return None and consume no slot.
    let manager = Arc::new(SubagentManager::new());
    let max = manager.max_concurrent();

    let _guards: Vec<_> = (0..max)
        .map(|_| {
            manager
                .try_acquire_slot()
                .expect("acquire should succeed within limit")
        })
        .collect();

    assert!(
        manager.try_acquire_slot().is_none(),
        "quota exhausted must return None"
    );
}

#[test]
fn test_current_depth_returns_zero_by_default() {
    // Root agent has depth 0 by default.
    let manager = SubagentManager::new();
    assert_eq!(manager.current_depth(), 0);
}

#[test]
fn test_current_depth_returns_set_value() {
    // set_depth() MUST be reflected by current_depth() — this is what
    // lets AgentTool::call enforce the max_depth limit.
    let manager = SubagentManager::new();
    manager.set_depth(2);
    assert_eq!(manager.current_depth(), 2);
}

#[tokio::test]
async fn test_depth_limit_exceeded_blocks_spawn() {
    // max_depth defaults to 3; a subagent at depth 3 cannot spawn
    // another child. AgentTool::call MUST return the depth error
    // before touching parent_config or the factory.
    let manager = Arc::new(SubagentManager::new());
    manager.set_depth(3);
    assert_eq!(manager.max_depth(), 3);
    let tool = AgentTool::new(manager, false);

    let input = test_tool_input(serde_json::json!({
        "description": "do something",
        "prompt": "do the thing",
        "subagent_type": "general"
    }));
    let output = tool.call(input).await;
    assert!(!output.is_text());
    let text = get_output_text(&output);
    assert!(
        text.contains("Max sub-agent depth reached"),
        "expected depth error, got: {}",
        text
    );
}

#[tokio::test]
async fn test_depth_limit_not_exceeded_allows_spawn() {
    // max_depth defaults to 3; a subagent at depth 2 is allowed to
    // spawn a child (depth 3). The depth check passes, so the call
    // proceeds until it hits the parent_config requirement (which is
    // not set in this test) — the error must NOT be the depth error.
    let manager = Arc::new(SubagentManager::new());
    manager.set_depth(2);
    assert_eq!(manager.max_depth(), 3);
    let tool = AgentTool::new(manager, false);

    let input = test_tool_input(serde_json::json!({
        "description": "do something",
        "prompt": "do the thing",
        "subagent_type": "general"
    }));
    let output = tool.call(input).await;
    assert!(!output.is_text());
    let text = get_output_text(&output);
    assert!(
        !text.contains("Max sub-agent depth reached"),
        "depth check should pass at depth 2 / max 3, got: {}",
        text
    );
    // Falls through to the parent_config requirement.
    assert!(text.contains("parent config"));
}

// ── Recursive subtree cancellation (spec: subagent-tree-cancellation) ──

/// Verify that `register_child_session` makes the child's token
/// reachable from `cancel_session_tree(parent_id)`.
///
/// Spec scenario: "Child session registered on creation".
#[tokio::test]
async fn test_register_child_session_adds_to_map() {
    let manager = SubagentManager::new();
    let parent_id = "parent-1".to_string();
    let child_id = "child-1".to_string();
    let child_token = CancellationToken::new();

    manager.register_child_session(
        parent_id.clone(),
        child_id.clone(),
        child_token.clone(),
    );

    // The child token must not be canceled yet.
    assert!(!child_token.is_cancelled());

    // Canceling the parent's tree must reach the registered child.
    manager.cancel_session_tree(&parent_id);
    assert!(
        child_token.is_cancelled(),
        "child token must be canceled after cancel_session_tree(parent)"
    );
}

/// Verify that `remove_session` cleans up both the parent's child list
/// and the session's own token entry, and cancels the removed session's
/// token (C1 fix: `remove_session` now cancels the token so any
/// still-running work is signaled).
///
/// Spec scenario: "Child registration cleaned up on session removal".
#[tokio::test]
async fn test_remove_session_cleans_up() {
    let manager = SubagentManager::new();
    let parent_id = "parent-2".to_string();
    let child_id = "child-2".to_string();
    let child_token = CancellationToken::new();

    manager.register_child_session(
        parent_id.clone(),
        child_id.clone(),
        child_token.clone(),
    );

    // remove_session cancels the removed session's token (C1 behavior:
    // the token is canceled so any still-running descendant is
    // signaled, and the entry is dropped from `session_cancel_tokens`).
    manager.remove_session(&child_id);
    assert!(
        child_token.is_cancelled(),
        "remove_session must cancel the removed session's token"
    );

    // cancel_session_tree on the parent must not panic and must be a
    // no-op for the already-removed child (entry is gone, so recursion
    // cannot revisit it).
    manager.cancel_session_tree(&parent_id);
}

/// Verify that `cancel_session_tree` recursively cancels all
/// descendants (children, grandchildren) before the target session.
///
/// Spec scenario: "Cancel parent cancels all descendants".
#[tokio::test]
async fn test_cancel_session_tree_cancels_descendants() {
    let manager = SubagentManager::new();
    let root = "root".to_string();
    let child_a = "child-a".to_string();
    let child_b = "child-b".to_string();
    let grandchild_a1 = "grandchild-a1".to_string();

    let root_token = CancellationToken::new();
    let child_a_token = CancellationToken::new();
    let child_b_token = CancellationToken::new();
    let grandchild_a1_token = CancellationToken::new();

    // Build the tree: root → [child_a → grandchild_a1, child_b]
    manager.register_child_session(
        "VIRTUAL_PARENT".to_string(),
        root.clone(),
        root_token.clone(),
    );
    manager.register_child_session(
        root.clone(),
        child_a.clone(),
        child_a_token.clone(),
    );
    manager.register_child_session(
        root.clone(),
        child_b.clone(),
        child_b_token.clone(),
    );
    manager.register_child_session(
        child_a.clone(),
        grandchild_a1.clone(),
        grandchild_a1_token.clone(),
    );

    assert!(!root_token.is_cancelled());
    assert!(!child_a_token.is_cancelled());
    assert!(!child_b_token.is_cancelled());
    assert!(!grandchild_a1_token.is_cancelled());

    manager.cancel_session_tree(&root);

    assert!(root_token.is_cancelled(), "root token must be canceled");
    assert!(
        child_a_token.is_cancelled(),
        "child_a token must be canceled"
    );
    assert!(
        child_b_token.is_cancelled(),
        "child_b token must be canceled"
    );
    assert!(
        grandchild_a1_token.is_cancelled(),
        "grandchild token must be canceled recursively"
    );
}

/// Verify that `cancel_session_tree` on a session with no children
/// cancels only the target session.
///
/// Spec scenario: "Cancel with no children".
#[tokio::test]
async fn test_cancel_session_tree_no_children() {
    let manager = SubagentManager::new();
    let parent_id = "parent-3".to_string();
    let child_id = "child-3".to_string();
    let other_id = "other-3".to_string();

    let child_token = CancellationToken::new();
    let other_token = CancellationToken::new();

    manager.register_child_session(
        parent_id.clone(),
        child_id.clone(),
        child_token.clone(),
    );
    manager.register_child_session(
        "VIRTUAL_PARENT".to_string(),
        other_id.clone(),
        other_token.clone(),
    );

    // child has no children of its own; canceling it must not touch
    // other.
    manager.cancel_session_tree(&child_id);
    assert!(
        child_token.is_cancelled(),
        "target child token must be canceled"
    );
    assert!(
        !other_token.is_cancelled(),
        "unrelated session token must not be canceled"
    );
}

/// Verify that `cancel_session_tree` cancels only the target subtree
/// and does not affect siblings.
///
/// Spec scenario: "Subtree cancel does not affect siblings".
#[tokio::test]
async fn test_cancel_session_tree_does_not_affect_siblings() {
    let manager = SubagentManager::new();
    let parent_id = "parent-4".to_string();
    let child_a = "child-a-4".to_string();
    let child_b = "child-b-4".to_string();
    let grandchild_a = "grandchild-a-4".to_string();

    let child_a_token = CancellationToken::new();
    let child_b_token = CancellationToken::new();
    let grandchild_a_token = CancellationToken::new();

    // parent → [child_a → grandchild_a, child_b]
    manager.register_child_session(
        parent_id.clone(),
        child_a.clone(),
        child_a_token.clone(),
    );
    manager.register_child_session(
        parent_id.clone(),
        child_b.clone(),
        child_b_token.clone(),
    );
    manager.register_child_session(
        child_a.clone(),
        grandchild_a.clone(),
        grandchild_a_token.clone(),
    );

    // Cancel child_a's subtree only.
    manager.cancel_session_tree(&child_a);

    assert!(
        child_a_token.is_cancelled(),
        "child_a token must be canceled"
    );
    assert!(
        grandchild_a_token.is_cancelled(),
        "grandchild of child_a must be canceled recursively"
    );
    assert!(
        !child_b_token.is_cancelled(),
        "sibling child_b must NOT be canceled"
    );
}

/// Verify that `cancel_session_tree` skips a concurrently-removed child
/// without panic and still cancels the remaining children.
///
/// Spec scenario: "Cancel handles concurrent child removal".
///
/// Note (C1 fix): `remove_session` now cancels the removed session's
/// token, so `child_a_token` IS cancelled by `remove_session` (not by
/// `cancel_session_tree`). The key property under test is that
/// `cancel_session_tree` does not panic on the missing entry and still
/// cancels the remaining child.
#[tokio::test]
async fn test_cancel_session_tree_skips_concurrent_removal() {
    let manager = SubagentManager::new();
    let parent_id = "parent-5".to_string();
    let child_a = "child-a-5".to_string();
    let child_b = "child-b-5".to_string();

    let child_a_token = CancellationToken::new();
    let child_b_token = CancellationToken::new();

    manager.register_child_session(
        parent_id.clone(),
        child_a.clone(),
        child_a_token.clone(),
    );
    manager.register_child_session(
        parent_id.clone(),
        child_b.clone(),
        child_b_token.clone(),
    );

    // Simulate concurrent removal of child_a before traversal reaches
    // it. The traversal collects the child list snapshot first, then
    // recurses; when it reaches child_a, the token entry is gone, so
    // the recursion is a no-op for that subtree. `remove_session` also
    // cancels child_a's token (C1 behavior).
    manager.remove_session(&child_a);

    // Must not panic.
    manager.cancel_session_tree(&parent_id);

    // child_a's token was cancelled by `remove_session` (not by the
    // tree cancellation), and its entry is gone so the tree
    // cancellation does not revisit it.
    assert!(
        child_a_token.is_cancelled(),
        "removed child_a token must be cancelled by remove_session"
    );
    // child_b is still registered, so it must be cancelled by the tree
    // cancellation.
    assert!(
        child_b_token.is_cancelled(),
        "remaining child_b token must still be cancelled"
    );
}

/// Verify that `remove_session` recursively cleans up all descendants.
///
/// C1 scenario: background nesting case where a grandchild session may
/// still be running when the child completes. `remove_session(child)`
/// MUST cancel the grandchild's token (so it stops) and drop its
/// tracking entries — otherwise the grandchild becomes an orphan that
/// `cancel_session_tree(root)` can no longer reach (memory leak +
/// cancellation semantics break).
///
/// Spec scenario: "Child registration cleaned up on session removal"
/// extended to the recursive/descendant case.
#[tokio::test]
async fn test_remove_session_cleans_up_descendants() {
    let manager = SubagentManager::new();
    let root = "root-d".to_string();
    let child = "child-d".to_string();
    let grandchild = "grandchild-d".to_string();

    let root_token = CancellationToken::new();
    let child_token = CancellationToken::new();
    let grandchild_token = CancellationToken::new();

    // Build the tree: root → child → grandchild.
    manager.register_child_session(
        "VIRTUAL_PARENT".to_string(),
        root.clone(),
        root_token.clone(),
    );
    manager.register_child_session(
        root.clone(),
        child.clone(),
        child_token.clone(),
    );
    manager.register_child_session(
        child.clone(),
        grandchild.clone(),
        grandchild_token.clone(),
    );

    assert!(!root_token.is_cancelled());
    assert!(!child_token.is_cancelled());
    assert!(!grandchild_token.is_cancelled());

    // Remove the middle session. This must recursively clean up
    // grandchild first (cancel its token + drop its entries), then
    // clean up child itself. Without the C1 fix, grandchild's token
    // would be orphaned — still uncancelled and unreachable from
    // cancel_session_tree(root) because child_sessions[child] = [grandchild]
    // would be dropped without recursing into grandchild.
    manager.remove_session(&child);

    // C1 core: grandchild's token MUST be cancelled by the recursive
    // remove_session, even though grandchild itself never completed.
    assert!(
        grandchild_token.is_cancelled(),
        "grandchild token must be cancelled by recursive remove_session"
    );

    // child's token MUST also be cancelled.
    assert!(
        child_token.is_cancelled(),
        "child token must be cancelled by remove_session"
    );

    // root's token must NOT be cancelled — remove_session only touches
    // the target session and its descendants, not the parent.
    assert!(
        !root_token.is_cancelled(),
        "root token must not be cancelled by removing a child"
    );

    // grandchild's entries MUST be removed from both tracking maps.
    assert!(
        !manager.has_child_session_entry(&grandchild),
        "grandchild must not have a child_sessions entry after recursive remove"
    );
    assert!(
        !manager.has_cancel_token_entry(&grandchild),
        "grandchild must not have a session_cancel_tokens entry after recursive remove"
    );

    // child's entries MUST be removed from both tracking maps.
    assert!(
        !manager.has_child_session_entry(&child),
        "child must not have a child_sessions entry after remove"
    );
    assert!(
        !manager.has_cancel_token_entry(&child),
        "child must not have a session_cancel_tokens entry after remove"
    );

    // root's child list MUST no longer contain child.
    assert!(
        !manager.children_of(&root).contains(&child),
        "root's child list must no longer contain the removed child"
    );

    // root's own token entry MUST still be intact (remove_session did
    // not incorrectly prune the parent).
    assert!(
        manager.has_cancel_token_entry(&root),
        "root token entry must be preserved after removing a child"
    );

    // cancel_session_tree(root) must still cancel root's token and must
    // not panic on the already-removed descendants.
    manager.cancel_session_tree(&root);
    assert!(
        root_token.is_cancelled(),
        "root token must be cancelled by cancel_session_tree after child removal"
    );
}
