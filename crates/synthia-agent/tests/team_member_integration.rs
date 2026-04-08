//! Integration tests for Team Member claiming and executing tasks
//!
//! Tests the complete flow of Team Member mode agent claiming and executing tasks.

use rmcp::model::CallToolResult;
use synthia_agent::tools::{
    Tool,
    task::{
        ClaimTaskFailureReason,
        ClaimTaskResult,
        ClaimTaskTool,
        Task,
        TaskPriority,
        TaskStatus,
    },
};

/// Helper to check if a result is successful
fn assert_success(result: &CallToolResult) {
    assert!(
        result.is_error.is_none() || result.is_error == Some(false),
        "Expected success but got error: {:?}",
        result
            .content
            .first()
            .and_then(|c| c.as_text().map(|t| &t.text))
    );
}

/// Helper to check if a result is an error
#[allow(dead_code)]
fn assert_error(result: &CallToolResult) {
    assert!(
        result.is_error == Some(true),
        "Expected error but got success"
    );
}

mod claim_task_tests {
    use super::*;

    #[tokio::test]
    async fn test_claim_task_not_found() {
        let tool = ClaimTaskTool::new();

        let args = serde_json::json!({
            "task_id": "nonexistent",
            "owner": "alice"
        });

        let result = tool.call(args).await;
        assert_success(&result); // Tool returns success with failure reason

        let content = &result.content[0];
        let text = content.as_text().unwrap();
        let claim_result: ClaimTaskResult =
            serde_json::from_str(&text.text).unwrap();
        assert!(!claim_result.success);
        assert_eq!(
            claim_result.reason,
            Some(ClaimTaskFailureReason::TaskNotFound)
        );
    }
}

mod task_model_tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new("task-1", "Test Task")
            .with_status(TaskStatus::Pending)
            .with_team("team-alpha");

        assert_eq!(task.id, "task-1");
        assert_eq!(task.subject, "Test Task");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.team_id, Some("team-alpha".to_string()));
    }

    #[test]
    fn test_task_with_priority() {
        let task = Task::new("task-1", "Test Task")
            .with_priority(TaskPriority::Critical);

        assert_eq!(task.priority, TaskPriority::Critical);
    }

    #[test]
    fn test_task_with_owner() {
        let task = Task::new("task-1", "Test Task").with_owner("alice");

        assert_eq!(task.owner, "alice");
    }

    #[test]
    fn test_task_status_transitions() {
        let mut task = Task::new("task-1", "Test Task");

        assert_eq!(task.status, TaskStatus::Pending);

        task.status = TaskStatus::InProgress;
        assert_eq!(task.status, TaskStatus::InProgress);

        task.status = TaskStatus::Completed;
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn test_task_dependencies() {
        let mut task = Task::new("task-1", "Test Task");
        task.blocked_by = vec!["dep-1".to_string(), "dep-2".to_string()];

        assert_eq!(task.blocked_by.len(), 2);
        assert!(task.blocked_by.contains(&"dep-1".to_string()));
        assert!(task.blocked_by.contains(&"dep-2".to_string()));
    }
}

mod claim_result_tests {
    use super::*;

    #[test]
    fn test_claim_result_success() {
        let result = ClaimTaskResult {
            success: true,
            task_id: Some("task-1".to_string()),
            reason: None,
            blocked_by_tasks: None,
            busy_with_tasks: None,
        };

        assert!(result.success);
        assert_eq!(result.task_id, Some("task-1".to_string()));
        assert!(result.reason.is_none());
    }

    #[test]
    fn test_claim_result_blocked() {
        let result = ClaimTaskResult {
            success: false,
            task_id: None,
            reason: Some(ClaimTaskFailureReason::Blocked),
            blocked_by_tasks: Some(vec!["dep-1".to_string()]),
            busy_with_tasks: None,
        };

        assert!(!result.success);
        assert_eq!(result.reason, Some(ClaimTaskFailureReason::Blocked));
        assert_eq!(result.blocked_by_tasks, Some(vec!["dep-1".to_string()]));
    }

    #[test]
    fn test_claim_result_already_claimed() {
        let result = ClaimTaskResult {
            success: false,
            task_id: None,
            reason: Some(ClaimTaskFailureReason::AlreadyClaimed),
            blocked_by_tasks: None,
            busy_with_tasks: None,
        };

        assert!(!result.success);
        assert_eq!(result.reason, Some(ClaimTaskFailureReason::AlreadyClaimed));
    }

    #[test]
    fn test_claim_result_agent_busy() {
        let result = ClaimTaskResult {
            success: false,
            task_id: None,
            reason: Some(ClaimTaskFailureReason::AgentBusy),
            blocked_by_tasks: None,
            busy_with_tasks: Some(vec!["task-other".to_string()]),
        };

        assert!(!result.success);
        assert_eq!(result.reason, Some(ClaimTaskFailureReason::AgentBusy));
        assert_eq!(
            result.busy_with_tasks,
            Some(vec!["task-other".to_string()])
        );
    }
}

mod failure_reason_tests {
    use super::*;

    #[test]
    fn test_all_failure_reasons() {
        // Verify all failure reasons are available
        let reasons = [
            ClaimTaskFailureReason::TaskNotFound,
            ClaimTaskFailureReason::AlreadyClaimed,
            ClaimTaskFailureReason::AlreadyResolved,
            ClaimTaskFailureReason::Blocked,
            ClaimTaskFailureReason::AgentBusy,
            ClaimTaskFailureReason::NoAvailableTasks,
        ];

        // Just verify they exist and can be compared
        assert_eq!(reasons[0], ClaimTaskFailureReason::TaskNotFound);
        assert_eq!(reasons[1], ClaimTaskFailureReason::AlreadyClaimed);
        assert_eq!(reasons[2], ClaimTaskFailureReason::AlreadyResolved);
        assert_eq!(reasons[3], ClaimTaskFailureReason::Blocked);
        assert_eq!(reasons[4], ClaimTaskFailureReason::AgentBusy);
        assert_eq!(reasons[5], ClaimTaskFailureReason::NoAvailableTasks);
    }
}
