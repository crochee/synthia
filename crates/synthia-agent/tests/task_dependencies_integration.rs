//! Integration tests for task dependencies and blocking
//!
//! Tests the complete flow of task dependency checking and blocking scenarios.

use synthia_agent::tools::task::{
    TaskGraph,
    TaskNode,
    TaskPriority,
    TaskStatus,
    TopologicalSortResult,
};

mod dependency_graph_tests {
    use super::*;

    #[test]
    fn test_simple_dependency_chain() {
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

        // Verify structure
        assert_eq!(graph.get_blocking_tasks("B"), vec!["A"]);
        assert_eq!(graph.get_blocking_tasks("C"), vec!["B"]);
        assert_eq!(graph.get_blocked_tasks("A"), vec!["B"]);
        assert_eq!(graph.get_blocked_tasks("B"), vec!["C"]);

        // Topological sort
        match graph.topological_sort() {
            TopologicalSortResult::Sorted(sorted) => {
                assert_eq!(sorted, vec!["A", "B", "C"]);
            }
            TopologicalSortResult::CycleDetected(_) => {
                panic!("Unexpected cycle");
            }
        }
    }

    #[test]
    fn test_parallel_tasks() {
        let mut graph = TaskGraph::new();

        // A and B are independent, both block C
        graph.add_task(TaskNode::new("A"));
        graph.add_task(TaskNode::new("B"));
        graph.add_task(
            TaskNode::new("C")
                .with_blocked_by(vec!["A".to_string(), "B".to_string()]),
        );
        graph.add_dependency("A", "C");
        graph.add_dependency("B", "C");

        // Both A and B should be ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&"A".to_string()));
        assert!(ready.contains(&"B".to_string()));

        // C should be blocked
        assert!(!graph.check_dependencies("C"));
    }

    #[test]
    fn test_diamond_dependency() {
        let mut graph = TaskGraph::new();

        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        graph.add_task(TaskNode::new("A"));
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("D")
                .with_blocked_by(vec!["B".to_string(), "C".to_string()]),
        );
        graph.add_dependency("A", "B");
        graph.add_dependency("A", "C");
        graph.add_dependency("B", "D");
        graph.add_dependency("C", "D");

        // Only A is ready initially
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"A".to_string()));

        // Complete A
        graph.update_status("A", TaskStatus::Completed);

        // Now B and C are ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 2);

        // Complete B and C
        graph.update_status("B", TaskStatus::Completed);
        graph.update_status("C", TaskStatus::Completed);

        // Now D is ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"D".to_string()));
    }

    #[test]
    fn test_complex_dependency_graph() {
        let mut graph = TaskGraph::new();

        // Complex graph:
        //     A   E
        //    /|   |
        //   B C   F
        //    \|   |
        //     D   G
        //      \ /
        //       H
        graph.add_task(TaskNode::new("A"));
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("D")
                .with_blocked_by(vec!["B".to_string(), "C".to_string()]),
        );
        graph.add_task(TaskNode::new("E"));
        graph.add_task(
            TaskNode::new("F").with_blocked_by(vec!["E".to_string()]),
        );
        graph.add_task(
            TaskNode::new("G").with_blocked_by(vec!["F".to_string()]),
        );
        graph.add_task(
            TaskNode::new("H")
                .with_blocked_by(vec!["D".to_string(), "G".to_string()]),
        );

        graph.add_dependency("A", "B");
        graph.add_dependency("A", "C");
        graph.add_dependency("B", "D");
        graph.add_dependency("C", "D");
        graph.add_dependency("E", "F");
        graph.add_dependency("F", "G");
        graph.add_dependency("D", "H");
        graph.add_dependency("G", "H");

        // A and E are ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&"A".to_string()));
        assert!(ready.contains(&"E".to_string()));

        // H is blocked by both D and G
        assert!(!graph.check_dependencies("H"));
    }
}

mod cycle_detection_tests {
    use super::*;

    #[test]
    fn test_simple_cycle() {
        let mut graph = TaskGraph::new();

        // A -> B -> A
        graph.add_task(
            TaskNode::new("A").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_dependency("B", "A");
        graph.add_dependency("A", "B");

        assert!(graph.has_cycle());

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 2);
    }

    #[test]
    fn test_three_node_cycle() {
        let mut graph = TaskGraph::new();

        // A -> B -> C -> A
        graph.add_task(
            TaskNode::new("A").with_blocked_by(vec!["C".to_string()]),
        );
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_dependency("C", "A");
        graph.add_dependency("A", "B");
        graph.add_dependency("B", "C");

        assert!(graph.has_cycle());

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 3);
    }

    #[test]
    fn test_no_cycle_in_valid_graph() {
        let mut graph = TaskGraph::new();

        graph.add_task(TaskNode::new("A"));
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_dependency("A", "B");
        graph.add_dependency("B", "C");

        assert!(!graph.has_cycle());
        assert!(graph.find_cycles().is_empty());
    }

    #[test]
    fn test_cycle_with_independent_branch() {
        let mut graph = TaskGraph::new();

        // Independent branch
        graph.add_task(TaskNode::new("X"));
        graph.add_task(
            TaskNode::new("Y").with_blocked_by(vec!["X".to_string()]),
        );
        graph.add_dependency("X", "Y");

        // Cycle
        graph.add_task(
            TaskNode::new("A").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_dependency("B", "A");
        graph.add_dependency("A", "B");

        assert!(graph.has_cycle());

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 2);
        assert!(!cycles.contains(&"X".to_string()));
        assert!(!cycles.contains(&"Y".to_string()));
    }
}

mod priority_with_dependencies_tests {
    use super::*;

    #[test]
    fn test_ready_tasks_sorted_by_priority() {
        let mut graph = TaskGraph::new();

        // Multiple independent tasks with different priorities
        graph.add_task(
            TaskNode::new("critical").with_priority(TaskPriority::Critical),
        );
        graph.add_task(TaskNode::new("low").with_priority(TaskPriority::Low));
        graph.add_task(TaskNode::new("high").with_priority(TaskPriority::High));
        graph.add_task(
            TaskNode::new("normal").with_priority(TaskPriority::Normal),
        );

        // Get ready tasks sorted by priority
        let ready = graph.get_ready_tasks_by_priority();
        assert_eq!(ready, vec!["critical", "high", "normal", "low"]);
    }

    #[test]
    fn test_priority_respects_dependencies() {
        let mut graph = TaskGraph::new();

        // High priority blocked by low priority
        graph.add_task(TaskNode::new("low").with_priority(TaskPriority::Low));
        graph.add_task(
            TaskNode::new("high")
                .with_priority(TaskPriority::High)
                .with_blocked_by(vec!["low".to_string()]),
        );
        graph.add_dependency("low", "high");

        // Only low is ready
        let ready = graph.get_ready_tasks_by_priority();
        assert_eq!(ready, vec!["low"]);

        // Complete low
        graph.update_status("low", TaskStatus::Completed);

        // Now high is ready
        let ready = graph.get_ready_tasks_by_priority();
        assert_eq!(ready, vec!["high"]);
    }
}

mod critical_path_tests {
    use super::*;

    #[test]
    fn test_critical_path_linear() {
        let mut graph = TaskGraph::new();

        graph.add_task(TaskNode::new("A"));
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_dependency("A", "B");
        graph.add_dependency("B", "C");

        let path = graph.get_critical_path();
        assert_eq!(path, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_critical_path_with_parallel_branches() {
        let mut graph = TaskGraph::new();

        //     A
        //    / \
        //   B   C
        //   |   |
        //   D   E
        //    \ /
        //     F
        graph.add_task(TaskNode::new("A"));
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("D").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_task(
            TaskNode::new("E").with_blocked_by(vec!["C".to_string()]),
        );
        graph.add_task(
            TaskNode::new("F")
                .with_blocked_by(vec!["D".to_string(), "E".to_string()]),
        );

        graph.add_dependency("A", "B");
        graph.add_dependency("A", "C");
        graph.add_dependency("B", "D");
        graph.add_dependency("C", "E");
        graph.add_dependency("D", "F");
        graph.add_dependency("E", "F");

        let path = graph.get_critical_path();
        // Critical path should be the longest path
        assert_eq!(path.len(), 4); // A -> (B or C) -> (D or E) -> F
    }

    #[test]
    fn test_tasks_by_depth() {
        let mut graph = TaskGraph::new();

        graph.add_task(TaskNode::new("A"));
        graph.add_task(
            TaskNode::new("B").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("C").with_blocked_by(vec!["A".to_string()]),
        );
        graph.add_task(
            TaskNode::new("D").with_blocked_by(vec!["B".to_string()]),
        );
        graph.add_dependency("A", "B");
        graph.add_dependency("A", "C");
        graph.add_dependency("B", "D");

        let depths = graph.get_tasks_by_depth();

        // Depth 0: A
        assert_eq!(depths[0], vec!["A"]);

        // Depth 1: B, C
        assert_eq!(depths[1].len(), 2);
        assert!(depths[1].contains(&"B".to_string()));
        assert!(depths[1].contains(&"C".to_string()));

        // Depth 2: D
        assert_eq!(depths[2], vec!["D"]);
    }
}

mod real_world_scenarios_tests {
    use super::*;

    #[test]
    fn test_feature_development_workflow() {
        // Create a realistic feature development workflow:
        // 1. Setup environment
        // 2. Design API (depends on 1)
        // 3. Implement backend (depends on 2)
        // 4. Implement frontend (depends on 2)
        // 5. Integration tests (depends on 3, 4)
        // 6. Documentation (depends on 3, 4)

        // Build graph
        let mut graph = TaskGraph::new();
        graph
            .add_task(TaskNode::new("setup").with_priority(TaskPriority::High));
        graph.add_task(
            TaskNode::new("design").with_priority(TaskPriority::Critical),
        );
        graph.add_task(
            TaskNode::new("backend").with_priority(TaskPriority::High),
        );
        graph.add_task(
            TaskNode::new("frontend").with_priority(TaskPriority::Normal),
        );
        graph.add_task(
            TaskNode::new("testing").with_priority(TaskPriority::Normal),
        );
        graph.add_task(TaskNode::new("docs").with_priority(TaskPriority::Low));

        graph.add_dependency("setup", "design");
        graph.add_dependency("design", "backend");
        graph.add_dependency("design", "frontend");
        graph.add_dependency("backend", "testing");
        graph.add_dependency("frontend", "testing");
        graph.add_dependency("backend", "docs");
        graph.add_dependency("frontend", "docs");

        // Verify no cycles
        assert!(!graph.has_cycle());

        // Only setup should be ready initially
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"setup".to_string()));

        // Complete setup
        graph.update_status("setup", TaskStatus::Completed);

        // Now design should be ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"design".to_string()));
    }

    #[test]
    fn test_task_blocking_chain() {
        let mut graph = TaskGraph::new();

        // Create a chain of tasks
        for i in 1..=5 {
            let blocked_by = if i > 1 {
                vec![format!("task-{}", i - 1)]
            } else {
                vec![]
            };
            graph.add_task(
                TaskNode::new(format!("task-{i}")).with_blocked_by(blocked_by),
            );
            if i > 1 {
                graph.add_dependency(
                    &format!("task-{}", i - 1),
                    &format!("task-{i}"),
                );
            }
        }

        // Only task-1 should be ready
        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"task-1".to_string()));

        // Complete tasks in order
        for i in 1..=5 {
            assert!(graph.check_dependencies(&format!("task-{i}")));
            graph.update_status(&format!("task-{i}"), TaskStatus::Completed);
        }
    }
}
