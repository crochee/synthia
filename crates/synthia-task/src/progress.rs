use std::collections::HashMap;

use crate::types::*;

pub fn report_progress(task: &Task) -> String {
    if task.progress.steps_total == 0 {
        return "No steps defined".to_string();
    }
    let pct = task.progress.steps_completed as f64
        / task.progress.steps_total as f64
        * 100.0;
    format!(
        "Task {}: {:.0}% complete ({}/{})",
        task.id, pct, task.progress.steps_completed, task.progress.steps_total
    )
}

/// Aggregate progress tracker for a set of tasks.
///
/// Tracks pending / completed / failed counts and updates
/// automatically when task state changes are recorded.
#[derive(Clone, Debug)]
pub struct ProgressTracker {
    /// All known tasks keyed by task id.
    tasks: HashMap<String, TaskStatus>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Register a new task as pending.
    pub fn add_task(&mut self, task_id: &str) {
        self.tasks.insert(task_id.to_string(), TaskStatus::Pending);
    }

    /// Update the status of a task and return the new aggregate snapshot.
    pub fn update(
        &mut self,
        task_id: &str,
        status: TaskStatus,
    ) -> Option<ProgressSnapshot> {
        self.tasks.insert(task_id.to_string(), status.clone());
        Some(self.snapshot())
    }

    /// Take a read-only snapshot of current aggregate counts.
    pub fn snapshot(&self) -> ProgressSnapshot {
        let mut pending = 0;
        let mut running = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut blocked = 0;

        for status in self.tasks.values() {
            match status {
                TaskStatus::Pending => pending += 1,
                TaskStatus::Running => running += 1,
                TaskStatus::Done => completed += 1,
                TaskStatus::Failed => failed += 1,
                TaskStatus::Blocked => blocked += 1,
            }
        }

        ProgressSnapshot {
            total: self.tasks.len(),
            pending,
            running,
            completed,
            failed,
            blocked,
        }
    }

    /// Return a human-readable summary.
    pub fn summary(&self) -> String {
        let s = self.snapshot();
        format!(
            "Total: {}, Pending: {}, Running: {}, Completed: {}, Failed: {}, Blocked: {}",
            s.total, s.pending, s.running, s.completed, s.failed, s.blocked
        )
    }

    /// Count of tasks in a specific status.
    pub fn count_by_status(&self, status: &TaskStatus) -> usize {
        self.tasks.values().filter(|s| *s == status).count()
    }

    /// Whether all tasks have reached a terminal state (Done or Failed).
    pub fn is_all_terminal(&self) -> bool {
        self.tasks
            .values()
            .all(|s| matches!(s, TaskStatus::Done | TaskStatus::Failed))
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only aggregate snapshot of task progress.
#[derive(Clone, Debug)]
pub struct ProgressSnapshot {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub blocked: usize,
}

impl ProgressSnapshot {
    /// Percentage of tasks that have completed successfully.
    pub fn completion_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.completed as f64 / self.total as f64) * 100.0
    }

    /// Percentage of tasks that have reached a terminal state.
    pub fn terminal_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        ((self.completed + self.failed) as f64 / self.total as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_progress() {
        let task = Task::new("t1".to_string(), 10);
        let report = report_progress(&task);
        assert!(report.contains("0%"));
    }

    #[test]
    fn test_report_progress_complete() {
        let mut task = Task::new("t1".to_string(), 5);
        task.start();
        task.progress.steps_completed = 5;
        let report = report_progress(&task);
        assert!(report.contains("100%"));
    }

    #[test]
    fn test_progress_tracker_add_task() {
        let mut tracker = ProgressTracker::new();
        tracker.add_task("task-1");
        tracker.add_task("task-2");

        let snap = tracker.snapshot();
        assert_eq!(snap.total, 2);
        assert_eq!(snap.pending, 2);
        assert_eq!(snap.completed, 0);
        assert_eq!(snap.failed, 0);
    }

    #[test]
    fn test_progress_tracker_update() {
        let mut tracker = ProgressTracker::new();
        tracker.add_task("task-1");
        tracker.add_task("task-2");
        tracker.add_task("task-3");

        tracker.update("task-1", TaskStatus::Done);
        tracker.update("task-2", TaskStatus::Running);

        let snap = tracker.snapshot();
        assert_eq!(snap.completed, 1);
        assert_eq!(snap.running, 1);
        assert_eq!(snap.pending, 1);
    }

    #[test]
    fn test_progress_tracker_summary() {
        let mut tracker = ProgressTracker::new();
        tracker.add_task("a");
        tracker.add_task("b");
        tracker.update("a", TaskStatus::Done);
        tracker.update("b", TaskStatus::Failed);

        let summary = tracker.summary();
        assert!(summary.contains("Completed: 1"));
        assert!(summary.contains("Failed: 1"));
    }

    #[test]
    fn test_progress_tracker_count_by_status() {
        let mut tracker = ProgressTracker::new();
        tracker.add_task("a");
        tracker.add_task("b");
        tracker.add_task("c");
        tracker.update("a", TaskStatus::Done);
        tracker.update("b", TaskStatus::Failed);

        assert_eq!(tracker.count_by_status(&TaskStatus::Done), 1);
        assert_eq!(tracker.count_by_status(&TaskStatus::Failed), 1);
        assert_eq!(tracker.count_by_status(&TaskStatus::Pending), 1);
    }

    #[test]
    fn test_progress_tracker_is_all_terminal() {
        let mut tracker = ProgressTracker::new();
        tracker.add_task("a");
        tracker.add_task("b");

        assert!(!tracker.is_all_terminal());

        tracker.update("a", TaskStatus::Done);
        assert!(!tracker.is_all_terminal());

        tracker.update("b", TaskStatus::Failed);
        assert!(tracker.is_all_terminal());
    }

    #[test]
    fn test_progress_snapshot_completion_pct() {
        let snap = ProgressSnapshot {
            total: 10,
            pending: 0,
            running: 0,
            completed: 3,
            failed: 2,
            blocked: 0,
        };
        assert!((snap.completion_pct() - 30.0).abs() < f64::EPSILON);
        assert!((snap.terminal_pct() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_snapshot_empty() {
        let snap = ProgressSnapshot {
            total: 0,
            pending: 0,
            running: 0,
            completed: 0,
            failed: 0,
            blocked: 0,
        };
        assert!((snap.completion_pct() - 0.0).abs() < f64::EPSILON);
        assert!((snap.terminal_pct() - 0.0).abs() < f64::EPSILON);
    }
}
