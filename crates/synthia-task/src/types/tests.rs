use super::*;

#[test]
fn test_task_creation() {
    let task = Task::new("t1".to_string(), 10);
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.progress.steps_total, 10);
    assert_eq!(task.progress.steps_completed, 0);
}

#[test]
fn test_task_start_and_complete() {
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    assert_eq!(task.status, TaskStatus::Running);
    task.complete();
    assert_eq!(task.status, TaskStatus::Done);
}

#[test]
fn test_progress_advance() {
    let mut progress = ProgressState::new(3);
    progress.advance();
    assert_eq!(progress.steps_completed, 1);
    progress.advance();
    progress.advance();
    assert!(progress.is_complete());
}

#[test]
fn test_task_fail() {
    let mut task = Task::new("t1".to_string(), 1);
    task.fail();
    assert_eq!(task.status, TaskStatus::Failed);
}

#[test]
fn test_structured_output() {
    let output = StructuredOutput {
        key: "result".to_string(),
        value: serde_json::json!("ok"),
    };
    assert_eq!(output.key, "result");
}

#[test]
fn test_task_status_state_machine_pending_to_running() {
    let mut task = Task::new("t1".to_string(), 5);
    assert!(task.start());
    assert_eq!(task.status, TaskStatus::Running);
}

#[test]
fn test_task_status_state_machine_cannot_start_twice() {
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    assert!(!task.start());
}

#[test]
fn test_task_status_state_machine_cannot_complete_from_pending() {
    let mut task = Task::new("t1".to_string(), 5);
    assert!(!task.complete());
}

#[test]
fn test_task_status_state_machine_cannot_complete_from_done() {
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    task.complete();
    assert!(!task.complete());
}

#[test]
fn test_task_status_state_machine_block_and_unblock() {
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    assert!(task.block());
    assert_eq!(task.status, TaskStatus::Blocked);
    assert!(task.unblock());
    assert_eq!(task.status, TaskStatus::Running);
}

#[test]
fn test_task_status_state_machine_cannot_block_from_pending() {
    let mut task = Task::new("t1".to_string(), 5);
    assert!(!task.block());
}

#[test]
fn test_task_status_state_machine_cannot_unblock_from_running() {
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    assert!(!task.unblock());
}

#[test]
fn test_task_fail_from_pending() {
    let mut task = Task::new("t1".to_string(), 5);
    assert!(task.fail());
    assert_eq!(task.status, TaskStatus::Failed);
}

#[test]
fn test_task_fail_from_running() {
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    assert!(task.fail());
    assert_eq!(task.status, TaskStatus::Failed);
}

#[test]
fn test_task_cannot_fail_from_done() {
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    task.complete();
    assert!(!task.fail());
}

#[test]
fn test_task_add_output() {
    let mut task = Task::new("t1".to_string(), 5);
    task.add_output(StructuredOutput {
        key: "result".to_string(),
        value: serde_json::json!("hello"),
    });
    assert_eq!(task.output.len(), 1);
    assert_eq!(task.output[0].key, "result");
}

#[test]
fn test_task_completion_percentage() {
    let mut task = Task::new("t1".to_string(), 10);
    assert!((task.completion_percentage() - 0.0).abs() < f64::EPSILON);

    task.progress.advance();
    assert!((task.completion_percentage() - 10.0).abs() < f64::EPSILON);

    task.progress.advance_by(4);
    assert!((task.completion_percentage() - 50.0).abs() < f64::EPSILON);

    task.progress.advance_by(100);
    assert!((task.completion_percentage() - 100.0).abs() < f64::EPSILON);
}

#[test]
fn test_progress_percentage_zero_steps() {
    let progress = ProgressState::new(0);
    assert!((progress.percentage() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_progress_advance_by() {
    let mut progress = ProgressState::new(10);
    progress.advance_by(3);
    assert_eq!(progress.steps_completed, 3);
    progress.advance_by(100);
    assert_eq!(progress.steps_completed, 10);
}

#[test]
fn test_task_with_owner_serializes_correctly() {
    let task =
        Task::new("t1".to_string(), 5).with_owner("agent-123".to_string());
    assert_eq!(task.owner, Some("agent-123".to_string()));

    let json = serde_json::to_string(&task).unwrap();
    assert!(json.contains("\"owner\":\"agent-123\""));

    let deserialized: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.owner, Some("agent-123".to_string()));
}

#[test]
fn test_task_owner_setter() {
    let mut task = Task::new("t1".to_string(), 5);
    assert!(task.owner.is_none());

    task.set_owner(Some("agent-456".to_string()));
    assert_eq!(task.owner, Some("agent-456".to_string()));

    task.set_owner(None);
    assert!(task.owner.is_none());
}

#[test]
fn test_old_json_without_owner_deserializes_with_none() {
    let old_json = serde_json::json!({
        "id": "old-task",
        "status": "Pending",
        "progress": {"steps_total": 3, "steps_completed": 0},
        "output": [],
        "notifications": []
    });

    let task: Task = serde_json::from_value(old_json).unwrap();
    assert_eq!(task.id, "old-task");
    assert!(task.owner.is_none());
}
