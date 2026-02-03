use super::data::BackgroundTask;
use crate::{
    Result,
    tools::storage::{FileStore, StoragePaths},
};

#[derive(Debug, Clone)]
pub(crate) struct BackgroundFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl BackgroundFileStore {
    pub(crate) fn new() -> Self {
        let paths = StoragePaths::new();
        let base = FileStore::new(paths.background_dir());

        Self { base, paths }
    }

    pub(crate) async fn create_task(&self, task: BackgroundTask) -> Result<()> {
        self.base.ensure_dir(&self.paths.background_dir()).await?;

        let tasks = self.load_tasks().await?;
        let mut tasks = tasks;
        tasks.push(task);

        self.save_tasks(&tasks).await
    }

    pub(crate) async fn get_task(
        &self,
        id: &str,
    ) -> Result<Option<BackgroundTask>> {
        let tasks = self.load_tasks().await?;
        Ok(tasks.into_iter().find(|t| t.id == id))
    }

    pub(crate) async fn update_task(
        &self,
        task: &BackgroundTask,
    ) -> Result<()> {
        let mut tasks = self.load_tasks().await?;

        if let Some(pos) = tasks.iter().position(|t| t.id == task.id) {
            tasks[pos] = task.clone();
            self.save_tasks(&tasks).await?;
        }

        Ok(())
    }

    pub(crate) async fn list_tasks(&self) -> Result<Vec<BackgroundTask>> {
        self.load_tasks().await
    }

    async fn load_tasks(&self) -> Result<Vec<BackgroundTask>> {
        let tasks_path = self.paths.background_tasks_file();

        if !self.base.file_exists(&tasks_path).await {
            return Ok(Vec::new());
        }

        self.base.read_json(&tasks_path).await
    }

    async fn save_tasks(&self, tasks: &[BackgroundTask]) -> Result<()> {
        let tasks_path = self.paths.background_tasks_file();
        self.base.write_json(&tasks_path, &tasks).await
    }
}

impl Default for BackgroundFileStore {
    fn default() -> Self {
        Self::new()
    }
}
