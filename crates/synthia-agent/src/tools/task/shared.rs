use super::{
    data::{Task, TaskStatus},
    file_store::TaskFileStore,
};
pub(crate) use crate::tools::shared::{err_result, ok_result, parse_args};

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub status: Option<TaskStatus>,
    pub owner: Option<String>,
}

impl TaskFilter {
    pub fn matches(&self, task: &Task) -> bool {
        let status_match =
            self.status.map(|s| task.status == s).unwrap_or(true);
        let owner_match = self
            .owner
            .as_ref()
            .map(|o| {
                task.owner == *o || (task.owner.is_empty() && o.is_empty())
            })
            .unwrap_or(true);
        status_match && owner_match
    }
}

pub async fn filter_tasks(
    store: &TaskFileStore,
    filter: TaskFilter,
) -> Result<Vec<Task>, String> {
    store
        .list_tasks()
        .await
        .map(|tasks| tasks.into_iter().filter(|t| filter.matches(t)).collect())
        .map_err(|e| format!("failed to list tasks: {e}"))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::tools::task::data::TaskPriority;

    fn make_task(id: &str, status: TaskStatus, owner: &str) -> Task {
        let task = Task::new(id, format!("Task {id}")).with_owner(owner);
        // Task::new defaults to Pending, override via status field directly
        let mut t = task;
        t.status = status;
        t
    }

    fn make_task_with_priority(
        id: &str,
        status: TaskStatus,
        owner: &str,
        priority: TaskPriority,
    ) -> Task {
        let task = Task::new(id, format!("Task {id}"))
            .with_owner(owner)
            .with_priority(priority);
        let mut t = task;
        t.status = status;
        t
    }

    // =====================================================================
    // TaskFilter::matches tests
    // =====================================================================

    #[test]
    fn test_task_filter_matches_no_filters() {
        let filter = TaskFilter::default();
        let task = make_task("1", TaskStatus::Pending, "alice");

        assert!(filter.matches(&task));
    }

    #[test]
    fn test_task_filter_matches_status_exact() {
        let filter = TaskFilter {
            status: Some(TaskStatus::Pending),
            owner: None,
        };
        let task = make_task("1", TaskStatus::Pending, "");

        assert!(filter.matches(&task));

        let task2 = make_task("2", TaskStatus::InProgress, "");
        assert!(!filter.matches(&task2));
    }

    #[test]
    fn test_task_filter_matches_all_statuses() {
        let statuses = [
            TaskStatus::Pending,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Stopped,
        ];

        for status in statuses {
            let filter = TaskFilter {
                status: Some(status),
                owner: None,
            };
            let task = make_task("1", status, "");
            assert!(
                filter.matches(&task),
                "Expected match for status {status:?}"
            );
        }
    }

    #[test]
    fn test_task_filter_matches_owner_exact() {
        let filter = TaskFilter {
            status: None,
            owner: Some("alice".to_string()),
        };
        let task = make_task("1", TaskStatus::Pending, "alice");

        assert!(filter.matches(&task));

        let task2 = make_task("2", TaskStatus::Pending, "bob");
        assert!(!filter.matches(&task2));
    }

    #[test]
    fn test_task_filter_matches_empty_owner_with_empty_filter() {
        let filter = TaskFilter {
            status: None,
            owner: Some("".to_string()),
        };
        let task = make_task("1", TaskStatus::Pending, "");

        assert!(filter.matches(&task));
    }

    #[test]
    fn test_task_filter_matches_empty_owner_with_none_filter() {
        // owner: None means "no filter" - should match any owner
        let filter = TaskFilter {
            status: None,
            owner: None,
        };
        let task1 = make_task("1", TaskStatus::Pending, "");
        let task2 = make_task("2", TaskStatus::Pending, "alice");

        assert!(filter.matches(&task1));
        assert!(filter.matches(&task2));
    }

    #[test]
    fn test_task_filter_matches_non_empty_owner_with_empty_task_owner() {
        let filter = TaskFilter {
            status: None,
            owner: Some("alice".to_string()),
        };
        let task = make_task("1", TaskStatus::Pending, "");

        // empty task owner should NOT match non-empty filter owner
        assert!(!filter.matches(&task));
    }

    #[test]
    fn test_task_filter_matches_status_and_owner_combined() {
        let filter = TaskFilter {
            status: Some(TaskStatus::InProgress),
            owner: Some("alice".to_string()),
        };

        let task1 = make_task("1", TaskStatus::InProgress, "alice");
        assert!(filter.matches(&task1));

        let task2 = make_task("2", TaskStatus::Pending, "alice");
        assert!(!filter.matches(&task2));

        let task3 = make_task("3", TaskStatus::InProgress, "bob");
        assert!(!filter.matches(&task3));

        let task4 = make_task("4", TaskStatus::Pending, "bob");
        assert!(!filter.matches(&task4));
    }

    #[test]
    fn test_task_filter_matches_status_terminal() {
        let filter = TaskFilter {
            status: Some(TaskStatus::Completed),
            owner: None,
        };

        let task = make_task("1", TaskStatus::Completed, "alice");
        assert!(filter.matches(&task));
    }

    #[test]
    fn test_task_filter_matches_with_priority_in_task() {
        // TaskFilter only filters on status and owner, not priority
        let filter = TaskFilter {
            status: Some(TaskStatus::Pending),
            owner: None,
        };

        let task1 = make_task_with_priority(
            "1",
            TaskStatus::Pending,
            "",
            TaskPriority::Low,
        );
        let task2 = make_task_with_priority(
            "2",
            TaskStatus::Pending,
            "",
            TaskPriority::Critical,
        );

        assert!(filter.matches(&task1));
        assert!(filter.matches(&task2));
    }

    #[test]
    fn test_task_filter_default_is_noop() {
        let filter = TaskFilter::default();

        let task = make_task("1", TaskStatus::Failed, "bob");
        assert!(filter.matches(&task));

        let task2 = make_task("2", TaskStatus::Stopped, "");
        assert!(filter.matches(&task2));
    }

    // =====================================================================
    // filter_tasks integration tests
    // =====================================================================

    #[tokio::test]
    async fn test_filter_tasks_empty_store() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let filter = TaskFilter::default();
        let result = filter_tasks(&store, filter).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_filter_tasks_no_filter() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = make_task("task-1", TaskStatus::Pending, "alice");
        let task2 = make_task("task-2", TaskStatus::InProgress, "bob");
        let task3 = make_task("task-3", TaskStatus::Completed, "alice");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();
        store.create_task(&task3).await.unwrap();

        let filter = TaskFilter::default();
        let result = filter_tasks(&store, filter).await.unwrap();

        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_filter_tasks_by_status() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = make_task("task-1", TaskStatus::Pending, "alice");
        let task2 = make_task("task-2", TaskStatus::InProgress, "bob");
        let task3 = make_task("task-3", TaskStatus::Completed, "alice");
        let task4 = make_task("task-4", TaskStatus::Pending, "carol");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();
        store.create_task(&task3).await.unwrap();
        store.create_task(&task4).await.unwrap();

        let filter = TaskFilter {
            status: Some(TaskStatus::Pending),
            owner: None,
        };
        let result = filter_tasks(&store, filter).await.unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|t| t.status == TaskStatus::Pending));
    }

    #[tokio::test]
    async fn test_filter_tasks_by_owner() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = make_task("task-1", TaskStatus::Pending, "alice");
        let task2 = make_task("task-2", TaskStatus::InProgress, "bob");
        let task3 = make_task("task-3", TaskStatus::Completed, "alice");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();
        store.create_task(&task3).await.unwrap();

        let filter = TaskFilter {
            status: None,
            owner: Some("alice".to_string()),
        };
        let result = filter_tasks(&store, filter).await.unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|t| t.owner == "alice"));
    }

    #[tokio::test]
    async fn test_filter_tasks_by_status_and_owner() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = make_task("task-1", TaskStatus::Pending, "alice");
        let task2 = make_task("task-2", TaskStatus::InProgress, "alice");
        let task3 = make_task("task-3", TaskStatus::Pending, "bob");
        let task4 = make_task("task-4", TaskStatus::InProgress, "bob");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();
        store.create_task(&task3).await.unwrap();
        store.create_task(&task4).await.unwrap();

        let filter = TaskFilter {
            status: Some(TaskStatus::Pending),
            owner: Some("alice".to_string()),
        };
        let result = filter_tasks(&store, filter).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "task-1");
    }

    #[tokio::test]
    async fn test_filter_tasks_empty_result() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = make_task("task-1", TaskStatus::Pending, "alice");
        let task2 = make_task("task-2", TaskStatus::InProgress, "bob");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();

        let filter = TaskFilter {
            status: Some(TaskStatus::Completed),
            owner: None,
        };
        let result = filter_tasks(&store, filter).await.unwrap();

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_filter_tasks_empty_owner_filter() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = make_task("task-1", TaskStatus::Pending, "");
        let task2 = make_task("task-2", TaskStatus::Pending, "alice");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();

        let filter = TaskFilter {
            status: None,
            owner: Some("".to_string()),
        };
        let result = filter_tasks(&store, filter).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "task-1");
    }

    #[tokio::test]
    async fn test_filter_tasks_preserves_task_data() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task = Task::new("task-1", "Test Task")
            .with_description("A description")
            .with_owner("alice")
            .with_priority(TaskPriority::High);

        store.create_task(&task).await.unwrap();

        let filter = TaskFilter {
            status: Some(TaskStatus::Pending),
            owner: Some("alice".to_string()),
        };
        let result = filter_tasks(&store, filter).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].description, "A description");
        assert_eq!(result[0].priority, TaskPriority::High);
    }

    // =====================================================================
    // TaskFilter Debug and Clone tests
    // =====================================================================

    #[test]
    fn test_task_filter_debug() {
        let filter = TaskFilter {
            status: Some(TaskStatus::Completed),
            owner: Some("alice".to_string()),
        };
        let debug = format!("{filter:?}");
        assert!(debug.contains("Completed"));
        assert!(debug.contains("alice"));
    }

    #[test]
    fn test_task_filter_clone() {
        let filter = TaskFilter {
            status: Some(TaskStatus::Pending),
            owner: Some("bob".to_string()),
        };
        let cloned = filter.clone();
        assert_eq!(filter.status, cloned.status);
        assert_eq!(filter.owner, cloned.owner);
    }

    #[test]
    fn test_task_filter_default() {
        let filter = TaskFilter::default();
        assert!(filter.status.is_none());
        assert!(filter.owner.is_none());
    }
}
