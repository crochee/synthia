//! Integration tests for Team Lead creating task graphs
//!
//! Tests the complete flow of Team Lead mode agent creating and managing task graphs.

use synthia_agent::tools::task::{
    Task,
    TaskGraph,
    TaskNode,
    TaskPriority,
    TaskStatus,
    TopologicalSortResult,
};

mod task_model_tests {
    use super::*;

    #[test]
    fn test_create_simple_task() {
        let task = Task::new("task-1", "Implement feature X");

        assert_eq!(task.id, "task-1");
        assert_eq!(task.subject, "Implement feature X");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.blocked_by.is_empty());
        assert!(task.blocks.is_empty());
    }

    #[test]
    fn test_create_task_with_priority() {
        let task = Task::new("task-1", "Critical bug fix")
            .with_priority(TaskPriority::Critical);

        assert_eq!(task.priority, TaskPriority::Critical);
    }

    #[test]
    fn test_create_task_with_team_id() {
        let task = Task::new("task-1", "Team task").with_team("team-alpha");

        assert_eq!(task.team_id, Some("team-alpha".to_string()));
    }

    #[test]
    fn test_create_task_with_owner() {
        let task = Task::new("task-1", "Assigned task").with_owner("alice");

        assert_eq!(task.owner, "alice");
    }
}

mod task_graph_tests {
    use super::*;

    #[test]
    fn test_create_task_with_dependencies() {
        let mut graph = TaskGraph::new();

        // Create chain: A -> B -> C
        graph.add_task(TaskNode::new("A"));
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_dependency("A", "B");
        graph.add_dependency("B", "C");

        // Verify dependency
        let blocking = graph.get_blocking_tasks("B");
        assert_eq!(blocking, vec!["A"]);

        let blocked = graph.get_blocked_tasks("A");
        assert_eq!(blocked, vec!["B"]);
    }

    #[test]
    fn test_task_graph_check_dependencies() {
        let mut graph = TaskGraph::new();

        // Add tasks with dependency
        graph.add_task(
            TaskNode::new("task-1").with_status(TaskStatus::Completed),
        );
        graph.add_task(
            TaskNode::new("task-2")
                .with_status(TaskStatus::Pending)
                .with_blocked_by(vec!["task-1".to_string()]),
        );
        graph.add_dependency("task-1", "task-2");

        // task-2 should be ready since task-1 is completed
        assert!(graph.check_dependencies("task-2"));

        // Add another incomplete dependency
        graph
            .add_task(TaskNode::new("task-3").with_status(TaskStatus::Pending));
        graph.add_dependency("task-3", "task-2");

        // Now task-2 should be blocked
        assert!(!graph.check_dependencies("task-2"));
    }

    #[test]
    fn test_task_graph_get_ready_tasks() {
        let mut graph = TaskGraph::new();

        // Add independent tasks
        graph.add_task(TaskNode::new("task-1"));
        graph.add_task(TaskNode::new("task-2"));
        graph.add_task(TaskNode::new("task-3"));

        // Add dependent task
        graph.add_task(
            TaskNode::new("task-4").with_blocked_by(vec!["task-1".to_string()]),
        );
        graph.add_dependency("task-1", "task-4");

        // Get ready tasks (should be task-1, task-2, task-3)
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 3);
        assert!(ready.contains(&"task-1".to_string()));
        assert!(ready.contains(&"task-2".to_string()));
        assert!(ready.contains(&"task-3".to_string()));
        assert!(!ready.contains(&"task-4".to_string()));
    }

    #[test]
    fn test_task_graph_topological_sort() {
        let mut graph = TaskGraph::new();

        // Create a linear dependency chain: task-1 -> task-2 -> task-3
        graph.add_task(TaskNode::new("task-1"));
        graph.add_task(
            TaskNode::new("task-2").with_blocked_by(vec!["task-1".to_string()]),
        );
        graph.add_task(
            TaskNode::new("task-3").with_blocked_by(vec!["task-2".to_string()]),
        );
        graph.add_dependency("task-1", "task-2");
        graph.add_dependency("task-2", "task-3");

        let result = graph.topological_sort();
        match result {
            TopologicalSortResult::Sorted(sorted) => {
                assert_eq!(sorted, vec!["task-1", "task-2", "task-3"]);
            }
            TopologicalSortResult::CycleDetected(_) => {
                panic!("Unexpected cycle detected");
            }
        }
    }

    #[test]
    fn test_task_graph_diamond_dependency() {
        let mut graph = TaskGraph::new();

        // Create diamond dependency:
        //       task-1
        //       /    \
        //   task-2  task-3
        //       \    /
        //       task-4
        graph.add_task(TaskNode::new("task-1"));
        graph.add_task(
            TaskNode::new("task-2").with_blocked_by(vec!["task-1".to_string()]),
        );
        graph.add_task(
            TaskNode::new("task-3").with_blocked_by(vec!["task-1".to_string()]),
        );
        graph.add_task(
            TaskNode::new("task-4").with_blocked_by(vec![
                "task-2".to_string(),
                "task-3".to_string(),
            ]),
        );
        graph.add_dependency("task-1", "task-2");
        graph.add_dependency("task-1", "task-3");
        graph.add_dependency("task-2", "task-4");
        graph.add_dependency("task-3", "task-4");

        // task-1 should be ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"task-1".to_string()));

        // Complete task-1
        graph.update_status("task-1", TaskStatus::Completed);

        // Now task-2 and task-3 should be ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 2);

        // Complete task-2 and task-3
        graph.update_status("task-2", TaskStatus::Completed);
        graph.update_status("task-3", TaskStatus::Completed);

        // Now task-4 should be ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"task-4".to_string()));
    }

    #[test]
    fn test_task_graph_cycle_detection() {
        let mut graph = TaskGraph::new();

        // Create a cycle: task-1 -> task-2 -> task-1
        graph.add_task(
            TaskNode::new("task-1").with_blocked_by(vec!["task-2".to_string()]),
        );
        graph.add_task(
            TaskNode::new("task-2").with_blocked_by(vec!["task-1".to_string()]),
        );
        graph.add_dependency("task-2", "task-1");
        graph.add_dependency("task-1", "task-2");

        assert!(graph.has_cycle());

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 2);
        assert!(cycles.contains(&"task-1".to_string()));
        assert!(cycles.contains(&"task-2".to_string()));
    }

    #[test]
    fn test_task_graph_priority_ordering() {
        let mut graph = TaskGraph::new();

        // Add tasks with different priorities
        graph.add_task(TaskNode::new("low").with_priority(TaskPriority::Low));
        graph.add_task(
            TaskNode::new("critical").with_priority(TaskPriority::Critical),
        );
        graph.add_task(TaskNode::new("high").with_priority(TaskPriority::High));
        graph.add_task(
            TaskNode::new("normal").with_priority(TaskPriority::Normal),
        );

        // Get ready tasks sorted by priority
        let ready = graph.get_ready_tasks_by_priority();
        assert_eq!(ready, vec!["critical", "high", "normal", "low"]);
    }
}

mod team_lead_workflow_tests {
    use super::*;

    #[test]
    fn test_lead_creates_task_graph_workflow() {
        // Step 1: Build task graph with dependencies
        let mut graph = TaskGraph::new();
        graph
            .add_task(TaskNode::new("setup").with_priority(TaskPriority::High));
        graph.add_task(
            TaskNode::new("implement").with_priority(TaskPriority::Normal),
        );
        graph.add_task(
            TaskNode::new("test").with_priority(TaskPriority::Normal),
        );

        // Setup -> Implement -> Test
        graph.add_dependency("setup", "implement");
        graph.add_dependency("implement", "test");

        // Step 2: Verify graph structure
        assert_eq!(graph.len(), 3);
        assert!(!graph.has_cycle());

        // Step 3: Get execution order
        let result = graph.topological_sort();
        match result {
            TopologicalSortResult::Sorted(sorted) => {
                assert_eq!(sorted, vec!["setup", "implement", "test"]);
            }
            TopologicalSortResult::CycleDetected(_) => {
                panic!("Unexpected cycle");
            }
        }

        // Step 4: Verify ready tasks
        let ready = graph.get_ready_tasks_by_priority();
        assert_eq!(ready, vec!["setup"]); // Only setup is ready initially
    }

    #[test]
    fn test_lead_assigns_tasks_to_team() {
        // Create tasks with team assignment
        let task1 = Task::new("task-1", "First task")
            .with_team("team-alpha")
            .with_priority(TaskPriority::High);

        let task2 = Task::new("task-2", "Second task")
            .with_team("team-alpha")
            .with_priority(TaskPriority::Normal);

        // Verify tasks are created with correct team
        assert_eq!(task1.team_id, Some("team-alpha".to_string()));
        assert_eq!(task2.team_id, Some("team-alpha".to_string()));
    }

    #[test]
    fn test_lead_tracks_task_progress() {
        // Create and track a task
        let mut task = Task::new("task-1", "Important task")
            .with_team("team-alpha")
            .with_owner("alice")
            .with_status(TaskStatus::InProgress);

        // Update task status
        task.status = TaskStatus::Completed;

        // Verify status update
        assert_eq!(task.status, TaskStatus::Completed);
    }
}
