use super::data::{Task, TaskPatch, TaskStatus};
use crate::{
    Result,
    tools::storage::{FileStore, Index, IndexEntry, StoragePaths},
};

#[derive(Debug, Clone)]
pub struct TaskSummary {
    pub id: String,
    pub subject: Option<String>,
    pub status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct TaskFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl TaskFileStore {
    pub fn new() -> Self {
        let paths = StoragePaths::new();
        let base = FileStore::new(paths.tasks_dir());

        Self { base, paths }
    }

    pub fn with_base(base_path: std::path::PathBuf) -> Self {
        let paths = StoragePaths::with_base(base_path);
        let base = FileStore::new(paths.tasks_dir());

        Self { base, paths }
    }

    pub async fn create_task(&self, task: &Task) -> Result<String> {
        self.base.ensure_dir(&self.paths.tasks_dir()).await?;

        let task_path = self.paths.task_file(&task.id);
        self.base.write_json(&task_path, task).await?;

        self.update_index(task).await?;

        Ok(task.id.clone())
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Task> {
        let task_path = self.paths.task_file(task_id);

        if !self.base.file_exists(&task_path).await {
            return Err(crate::AgentError::session(format!(
                "Task not found: {task_id}"
            )));
        }

        self.base.read_json(&task_path).await
    }

    pub async fn list_tasks(&self) -> Result<Vec<Task>> {
        let tasks_dir = self.paths.tasks_dir();

        if !tasks_dir.exists() {
            return Ok(Vec::new());
        }

        let files = self.base.list_files(&tasks_dir, "json").await?;

        let mut tasks = Vec::new();
        for file in files {
            if file.file_name().is_some_and(|name| name == "index.json") {
                continue;
            }

            match self.base.read_json::<Task>(&file).await {
                Ok(task) => tasks.push(task),
                Err(e) => {
                    tracing::warn!(
                        "Failed to load task from {:?}: {}",
                        file,
                        e
                    );
                }
            }
        }

        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(tasks)
    }

    pub async fn list_task_summaries(&self) -> Result<Vec<TaskSummary>> {
        let index_path = self.paths.task_index();

        if !self.base.file_exists(&index_path).await {
            return Ok(Vec::new());
        }

        let index: Index = self.base.read_json(&index_path).await?;
        let mut items = index.items;
        items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let summaries: Vec<TaskSummary> = items
            .into_iter()
            .map(|e| TaskSummary {
                id: e.id,
                subject: e.subject,
                status: e.status,
                updated_at: e.updated_at,
            })
            .collect();

        Ok(summaries)
    }

    pub async fn get_task_summary(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskSummary>> {
        let index_path = self.paths.task_index();

        if !self.base.file_exists(&index_path).await {
            return Ok(None);
        }

        let index: Index = self.base.read_json(&index_path).await?;

        if let Some(entry) = index.get_entry(task_id) {
            Ok(Some(TaskSummary {
                id: entry.id.clone(),
                subject: entry.subject.clone(),
                status: entry.status.clone(),
                updated_at: entry.updated_at,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn update_task(
        &self,
        task_id: &str,
        patch: &TaskPatch,
    ) -> Result<Task> {
        let mut task = self.get_task(task_id).await?;

        if let Some(ref subject) = patch.subject {
            task.subject = subject.clone();
        }
        if let Some(ref description) = patch.description {
            task.description = description.clone();
        }
        if let Some(status) = patch.status {
            task.status = status;
        }
        if let Some(ref blocked_by) = patch.blocked_by {
            task.blocked_by = blocked_by.clone();
        }
        if let Some(ref blocks) = patch.blocks {
            task.blocks = blocks.clone();
        }
        if let Some(ref owner) = patch.owner {
            task.owner = owner.clone();
        }
        if let Some(ref team_id) = patch.team_id {
            task.team_id = Some(team_id.clone());
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(deadline) = patch.deadline {
            task.deadline = Some(deadline);
        }
        if let Some(ref output) = patch.output {
            task.output = output.clone();
        }

        task.touch();

        let task_path = self.paths.task_file(task_id);
        self.base.write_json(&task_path, &task).await?;

        self.update_index(&task).await?;

        Ok(task)
    }

    pub async fn delete_task(&self, task_id: &str) -> Result<()> {
        let task_path = self.paths.task_file(task_id);
        self.base.delete_file(&task_path).await?;

        self.remove_from_index(task_id).await?;

        Ok(())
    }

    pub async fn unassign_tasks(&self, owner: &str) -> Result<Vec<Task>> {
        let tasks = self.list_tasks().await?;

        let mut unassigned = Vec::new();
        for task in tasks {
            if task.owner == owner && !task.status.is_terminal() {
                let patch = TaskPatch::new()
                    .with_owner("")
                    .with_status(TaskStatus::Pending);
                let updated = self.update_task(&task.id, &patch).await?;
                unassigned.push(updated);
            }
        }

        Ok(unassigned)
    }

    pub async fn append_task_output(
        &self,
        task_id: &str,
        output: &str,
    ) -> Result<Task> {
        let mut task = self.get_task(task_id).await?;
        task.append_output(output);

        let task_path = self.paths.task_file(task_id);
        self.base.write_json(&task_path, &task).await?;

        self.update_index(&task).await?;

        Ok(task)
    }

    pub async fn add_task_message(
        &self,
        task_id: &str,
        role: &str,
        content: &str,
    ) -> Result<Task> {
        let mut task = self.get_task(task_id).await?;
        task.add_message(role, content);

        let task_path = self.paths.task_file(task_id);
        self.base.write_json(&task_path, &task).await?;

        self.update_index(&task).await?;

        Ok(task)
    }

    async fn update_index(&self, task: &Task) -> Result<()> {
        let index_path = self.paths.task_index();

        let mut index = if self.base.file_exists(&index_path).await {
            self.base.read_json(&index_path).await.unwrap_or_default()
        } else {
            Index::new()
        };

        let entry = IndexEntry {
            id: task.id.clone(),
            subject: Some(task.subject.clone()),
            status: task.status.as_str().to_string(),
            updated_at: task.updated_at,
        };

        index.add_entry(entry);

        self.base.write_json(&index_path, &index).await
    }

    async fn remove_from_index(&self, task_id: &str) -> Result<()> {
        let index_path = self.paths.task_index();

        if !self.base.file_exists(&index_path).await {
            return Ok(());
        }

        let mut index: Index = self.base.read_json(&index_path).await?;
        index.remove_entry(task_id);

        self.base.write_json(&index_path, &index).await
    }

    pub async fn batch_update(
        &self,
        updates: Vec<(Task, TaskPatch)>,
    ) -> Result<Vec<Task>> {
        let tasks = self.list_tasks().await?;
        let mut updated = Vec::new();
        for task in tasks {
            if let Some(update) = updates.iter().find(|(t, _)| t.id == task.id)
            {
                let (_, patch) = update;
                let updated_task = self.update_task(&task.id, patch).await?;
                updated.push(updated_task);
            }
        }

        self.update_index_for_batch(&updated).await?;
        Ok(updated)
    }

    async fn update_index_for_batch(&self, tasks: &[Task]) -> Result<()> {
        let index_path = self.paths.task_index();

        let mut index = if self.base.file_exists(&index_path).await {
            self.base.read_json(&index_path).await?
        } else {
            Index::new()
        };

        for task in tasks {
            let entry = IndexEntry {
                id: task.id.clone(),
                subject: Some(task.subject.clone()),
                status: task.status.as_str().to_string(),
                updated_at: task.updated_at,
            };
            index.add_entry(entry);
        }

        self.base.write_json(&index_path, &index).await
    }
}

impl Default for TaskFileStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn test_task_file_store_create_and_get() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task = Task::new("task-1", "Test Task")
            .with_description("Test description")
            .with_owner("agent-1");

        store.create_task(&task).await.unwrap();

        let loaded = store.get_task("task-1").await.unwrap();

        assert_eq!(task.id, loaded.id);
        assert_eq!(task.subject, loaded.subject);
        assert_eq!(task.description, loaded.description);
        assert_eq!(task.owner, loaded.owner);
    }

    #[tokio::test]
    async fn test_task_file_store_list() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = Task::new("task-1", "Task 1");
        let task2 = Task::new("task-2", "Task 2");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();

        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[tokio::test]
    async fn test_task_file_store_update() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task = Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let patch = TaskPatch::new()
            .with_status(TaskStatus::InProgress)
            .with_owner("agent-2");

        let updated = store.update_task("task-1", &patch).await.unwrap();

        assert_eq!(updated.status, TaskStatus::InProgress);
        assert_eq!(updated.owner, "agent-2");
    }

    #[tokio::test]
    async fn test_task_file_store_delete() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task = Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        store.delete_task("task-1").await.unwrap();

        let result = store.get_task("task-1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_file_store_append_output() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task = Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let updated = store
            .append_task_output("task-1", "Output line 1\n")
            .await
            .unwrap();
        assert_eq!(updated.output, "Output line 1\n");

        let updated = store
            .append_task_output("task-1", "Output line 2\n")
            .await
            .unwrap();
        assert_eq!(updated.output, "Output line 1\nOutput line 2\n");
    }

    #[tokio::test]
    async fn test_task_file_store_add_message() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task = Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let updated = store
            .add_task_message("task-1", "user", "Hello")
            .await
            .unwrap();
        assert_eq!(updated.messages.len(), 1);
        assert_eq!(updated.messages[0].role, "user");
        assert_eq!(updated.messages[0].content, "Hello");
    }

    #[tokio::test]
    async fn test_task_file_store_unassign_tasks() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = Task::new("task-1", "Task 1").with_owner("agent-1");
        let task2 = Task::new("task-2", "Task 2").with_owner("agent-1");
        let task3 = Task::new("task-3", "Task 3").with_owner("agent-2");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();
        store.create_task(&task3).await.unwrap();

        let unassigned = store.unassign_tasks("agent-1").await.unwrap();
        assert_eq!(unassigned.len(), 2);

        let task1 = store.get_task("task-1").await.unwrap();
        assert!(task1.owner.is_empty());
        assert_eq!(task1.status, TaskStatus::Pending);

        let task3 = store.get_task("task-3").await.unwrap();
        assert_eq!(task3.owner, "agent-2");
    }

    #[tokio::test]
    async fn test_task_file_store_list_summaries() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task1 = Task::new("task-1", "Task 1");
        let task2 = Task::new("task-2", "Task 2");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();

        let summaries = store.list_task_summaries().await.unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, "task-1");
        assert_eq!(summaries[1].id, "task-2");
    }

    #[tokio::test]
    async fn test_task_file_store_get_summary() {
        let dir = tempdir().unwrap();
        let store = TaskFileStore::with_base(dir.path().to_path_buf());

        let task = Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let summary = store.get_task_summary("task-1").await.unwrap();
        assert!(summary.is_some());
        let s = summary.unwrap();
        assert_eq!(s.id, "task-1");
        assert_eq!(s.subject, Some("Test Task".to_string()));

        let not_found = store.get_task_summary("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }
}
