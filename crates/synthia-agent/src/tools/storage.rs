use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::{fs, io::AsyncWriteExt};

use crate::{AgentError, Result};

#[derive(Debug, Clone)]
pub struct FileStore {
    base_path: Arc<PathBuf>,
}

impl FileStore {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path: Arc::new(base_path),
        }
    }

    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    pub async fn ensure_dir(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            fs::create_dir_all(path).await.map_err(|e| {
                AgentError::internal(format!(
                    "Failed to create directory '{}': {}",
                    path.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    pub async fn read_json<T: DeserializeOwned>(
        &self,
        path: &Path,
    ) -> Result<T> {
        let content = fs::read_to_string(path).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to read file '{}': {e}",
                path.display()
            ))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            AgentError::internal(format!(
                "Failed to parse JSON from '{}': {}",
                path.display(),
                e
            ))
        })
    }

    pub async fn write_json<T: Serialize + Sync>(
        &self,
        path: &Path,
        data: &T,
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            self.ensure_dir(parent).await?;
        }

        let content = serde_json::to_string_pretty(data).map_err(|e| {
            AgentError::internal(format!("Failed to serialize JSON: {e}"))
        })?;

        self.atomic_write(path, &content).await
    }

    pub async fn append_jsonl<T: Serialize + Sync>(
        &self,
        path: &Path,
        data: &T,
    ) -> Result<()> {
        let mut line = serde_json::to_string(data).map_err(|e| {
            AgentError::internal(format!("Failed to serialize JSONL: {e}"))
        })?;
        line.push('\n');

        if let Some(parent) = path.parent() {
            self.ensure_dir(parent).await?;
        }

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(|e| {
                AgentError::internal(format!(
                    "Failed to open file '{}' for append: {e}",
                    path.display()
                ))
            })?;

        file.write_all(line.as_bytes()).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to append to file '{}': {e}",
                path.display()
            ))
        })?;

        file.sync_data().await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to sync file '{}': {e}",
                path.display()
            ))
        })?;

        Ok(())
    }

    pub async fn read_jsonl<T: DeserializeOwned>(
        &self,
        path: &Path,
    ) -> Result<Vec<T>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to read file '{}': {}",
                path.display(),
                e
            ))
        })?;

        let mut items = Vec::new();
        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<T>(line) {
                Ok(item) => items.push(item),
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse JSONL line {} in '{}': {}",
                        line_num + 1,
                        path.display(),
                        e
                    );
                }
            }
        }

        Ok(items)
    }

    pub async fn atomic_write(&self, path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            self.ensure_dir(parent).await?;
        }

        let temp_path = path.with_extension("tmp");

        {
            let mut file = fs::File::create(&temp_path).await.map_err(|e| {
                AgentError::internal(format!(
                    "Failed to create temp file '{}': {e}",
                    temp_path.display()
                ))
            })?;

            file.write_all(content.as_bytes()).await.map_err(|e| {
                AgentError::internal(format!(
                    "Failed to write to temp file '{}': {e}",
                    temp_path.display()
                ))
            })?;

            file.sync_data().await.map_err(|e| {
                AgentError::internal(format!(
                    "Failed to sync temp file '{}': {e}",
                    temp_path.display()
                ))
            })?;
        }

        fs::rename(&temp_path, path).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to rename '{}' to '{}': {e}",
                temp_path.display(),
                path.display()
            ))
        })?;

        Ok(())
    }

    pub async fn delete_file(&self, path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_file(path).await.map_err(|e| {
                AgentError::internal(format!(
                    "Failed to delete file '{}': {e}",
                    path.display()
                ))
            })?;
        }
        Ok(())
    }

    pub async fn file_exists(&self, path: &Path) -> bool {
        path.exists() && path.is_file()
    }

    pub async fn list_files(
        &self,
        dir: &Path,
        extension: &str,
    ) -> Result<Vec<PathBuf>> {
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(dir).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to read directory '{}': {e}",
                dir.display()
            ))
        })?;

        let mut files = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AgentError::internal(format!("Failed to read directory entry: {e}"))
        })? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == extension) {
                files.push(path);
            }
        }

        Ok(files)
    }
}

#[derive(Debug, Clone)]
pub struct StoragePaths {
    base_data_dir: PathBuf,
}

impl StoragePaths {
    pub fn new() -> Self {
        // Use .agents directory in current working directory for project-local storage
        Self {
            base_data_dir: PathBuf::from(".agents"),
        }
    }

    pub fn with_base(base: PathBuf) -> Self {
        // with_base keeps backwards compatibility: <base>/data
        Self {
            base_data_dir: base.join("data"),
        }
    }

    pub fn tasks_dir(&self) -> PathBuf {
        self.base_data_dir.join("tasks")
    }

    pub fn task_file(&self, task_id: &str) -> PathBuf {
        self.tasks_dir().join(format!("{task_id}.json"))
    }

    pub fn task_index(&self) -> PathBuf {
        self.tasks_dir().join("index.json")
    }

    pub fn teammates_dir(&self) -> PathBuf {
        self.base_data_dir.join("teammates")
    }

    pub fn teammate_file(&self, name: &str) -> PathBuf {
        self.teammates_dir().join(format!("{name}.json"))
    }

    pub fn teammate_index(&self) -> PathBuf {
        self.teammates_dir().join("index.json")
    }

    pub fn teams_dir(&self) -> PathBuf {
        self.base_data_dir.join("teams")
    }

    pub fn team_file(&self, team_id: &str) -> PathBuf {
        self.teams_dir().join(format!("{team_id}.json"))
    }

    pub fn team_index(&self) -> PathBuf {
        self.teams_dir().join("index.json")
    }

    pub fn messages_dir(&self) -> PathBuf {
        self.base_data_dir.join("messages")
    }

    pub fn message_file(&self, recipient: &str) -> PathBuf {
        self.messages_dir().join(format!("{recipient}.jsonl"))
    }

    pub fn cron_dir(&self) -> PathBuf {
        self.base_data_dir.join("cron")
    }

    pub fn cron_jobs_file(&self) -> PathBuf {
        self.cron_dir().join("jobs.json")
    }

    pub fn cron_runs_dir(&self) -> PathBuf {
        self.cron_dir().join("runs")
    }

    pub fn cron_runs_file(&self, job_id: &str) -> PathBuf {
        self.cron_runs_dir().join(format!("{job_id}.jsonl"))
    }

    pub fn background_dir(&self) -> PathBuf {
        self.base_data_dir.join("background")
    }

    pub fn background_tasks_file(&self) -> PathBuf {
        self.background_dir().join("tasks.json")
    }

    pub fn protocol_dir(&self) -> PathBuf {
        self.base_data_dir.join("protocol")
    }

    pub fn shutdown_requests_file(&self) -> PathBuf {
        self.protocol_dir().join("shutdown_requests.json")
    }

    pub fn plan_requests_file(&self) -> PathBuf {
        self.protocol_dir().join("plan_requests.json")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.base_data_dir.join("sessions")
    }

    pub fn session_file(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{session_id}.json"))
    }

    pub fn session_messages_file(&self, session_id: &str) -> PathBuf {
        self.sessions_dir()
            .join(format!("{session_id}_messages.jsonl"))
    }

    pub fn sessions_index_file(&self) -> PathBuf {
        self.sessions_dir().join("index.json")
    }
}

impl Default for StoragePaths {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub updated_at: i64,
    pub items: Vec<IndexEntry>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at: chrono::Utc::now().timestamp(),
            items: Vec::new(),
        }
    }
}

impl Index {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_entry(&mut self, entry: IndexEntry) {
        if let Some(pos) = self.items.iter().position(|e| e.id == entry.id) {
            self.items[pos] = entry;
        } else {
            self.items.push(entry);
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn remove_entry(&mut self, id: &str) {
        self.items.retain(|e| e.id != id);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn get_entry(&self, id: &str) -> Option<&IndexEntry> {
        self.items.iter().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn test_file_store_write_read_json() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("test.json");

        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestData {
            name: String,
            value: i32,
        }

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        store.write_json(&path, &data).await.unwrap();
        let loaded: TestData = store.read_json(&path).await.unwrap();

        assert_eq!(data, loaded);
    }

    #[tokio::test]
    async fn test_file_store_read_json_missing_file() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("nonexistent.json");

        #[derive(Deserialize, Debug)]
        struct TestData {}

        let result: Result<TestData, _> = store.read_json(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_store_read_json_malformed() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("malformed.json");

        fs::write(&path, "{ invalid json }").await.unwrap();

        #[derive(Deserialize, Debug)]
        struct TestData {}

        let result: Result<TestData, _> = store.read_json(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_store_append_read_jsonl() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("test.jsonl");

        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestItem {
            id: u32,
            name: String,
        }

        let items = vec![
            TestItem {
                id: 1,
                name: "first".to_string(),
            },
            TestItem {
                id: 2,
                name: "second".to_string(),
            },
        ];

        for item in &items {
            store.append_jsonl(&path, item).await.unwrap();
        }

        let loaded: Vec<TestItem> = store.read_jsonl(&path).await.unwrap();
        assert_eq!(items, loaded);
    }

    #[tokio::test]
    async fn test_file_store_read_jsonl_missing_file() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("nonexistent.jsonl");

        #[derive(Deserialize, Debug)]
        struct TestItem {}

        let result: Vec<TestItem> = store.read_jsonl(&path).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_file_store_read_jsonl_malformed_lines() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("malformed.jsonl");

        let content = r#"{"id": 1, "name": "valid"}
{ invalid json line }
{"id": 3, "name": "also_valid"}
"#;
        fs::write(&path, content).await.unwrap();

        #[derive(Deserialize, Debug, PartialEq)]
        struct TestItem {
            id: u32,
            name: String,
        }

        let result: Vec<TestItem> = store.read_jsonl(&path).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[1].id, 3);
    }

    #[tokio::test]
    async fn test_file_store_read_jsonl_empty_lines() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("empty_lines.jsonl");

        let content = "{\"id\": 1, \"name\": \"first\"}\n\n{\"id\": 2, \"name\": \"second\"}\n   \n";
        fs::write(&path, content).await.unwrap();

        #[derive(Deserialize, Debug, PartialEq)]
        struct TestItem {
            id: u32,
            name: String,
        }

        let result: Vec<TestItem> = store.read_jsonl(&path).await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_file_store_atomic_write() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("test.txt");

        let content = "Hello, World!";
        store.atomic_write(&path, content).await.unwrap();

        let loaded = fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, loaded);
    }

    #[tokio::test]
    async fn test_file_store_atomic_write_nested_dir() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("nested").join("deep").join("test.txt");

        store.atomic_write(&path, "content").await.unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_file_store_ensure_dir() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let subdir = dir.path().join("subdir").join("nested");

        store.ensure_dir(&subdir).await.unwrap();
        assert!(subdir.exists());
        assert!(subdir.is_dir());
    }

    #[tokio::test]
    async fn test_file_store_ensure_dir_existing() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());

        store.ensure_dir(dir.path()).await.unwrap();
        assert!(dir.path().exists());
    }

    #[tokio::test]
    async fn test_file_store_delete_file() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("to_delete.txt");

        fs::write(&path, "content").await.unwrap();
        assert!(path.exists());

        store.delete_file(&path).await.unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_file_store_delete_file_nonexistent() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("nonexistent.txt");

        let result = store.delete_file(&path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_store_file_exists() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let existing = dir.path().join("exists.txt");
        let nonexistent = dir.path().join("does_not_exist.txt");

        fs::write(&existing, "content").await.unwrap();

        assert!(store.file_exists(&existing).await);
        assert!(!store.file_exists(&nonexistent).await);
    }

    #[tokio::test]
    async fn test_file_store_list_files() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());

        fs::write(dir.path().join("file1.json"), "{}")
            .await
            .unwrap();
        fs::write(dir.path().join("file2.json"), "{}")
            .await
            .unwrap();
        fs::write(dir.path().join("file.txt"), "text")
            .await
            .unwrap();
        fs::write(dir.path().join("file2.yaml"), "key: value")
            .await
            .unwrap();

        let json_files = store.list_files(dir.path(), "json").await.unwrap();
        assert_eq!(json_files.len(), 2);

        let yaml_files = store.list_files(dir.path(), "yaml").await.unwrap();
        assert_eq!(yaml_files.len(), 1);
    }

    #[tokio::test]
    async fn test_file_store_list_files_nonexistent_dir() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let nonexistent = dir.path().join("nonexistent");

        let result = store.list_files(&nonexistent, "json").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_file_store_list_files_empty_dir() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());

        let result = store.list_files(dir.path(), "json").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_file_store_write_json_nested_dir() {
        let dir = tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let path = dir.path().join("nested").join("data.json");

        #[derive(Serialize, Deserialize)]
        struct TestData {
            value: i32,
        }

        store
            .write_json(&path, &TestData { value: 99 })
            .await
            .unwrap();
        assert!(path.exists());

        let loaded: TestData = store.read_json(&path).await.unwrap();
        assert_eq!(loaded.value, 99);
    }

    #[tokio::test]
    async fn test_storage_paths() {
        let paths = StoragePaths::new();

        assert!(paths.tasks_dir().ends_with("tasks"));
        assert!(paths.task_file("123").ends_with("123.json"));
        assert!(paths.task_index().ends_with("index.json"));
    }

    #[test]
    fn test_storage_paths_with_base() {
        let base = PathBuf::from("/custom/base");
        let paths = StoragePaths::with_base(base.clone());

        assert_eq!(paths.tasks_dir(), base.join("data").join("tasks"));
        assert_eq!(paths.teammates_dir(), base.join("data").join("teammates"));
        assert_eq!(paths.teams_dir(), base.join("data").join("teams"));
        assert_eq!(paths.messages_dir(), base.join("data").join("messages"));
        assert_eq!(paths.cron_dir(), base.join("data").join("cron"));
        assert_eq!(
            paths.background_dir(),
            base.join("data").join("background")
        );
        assert_eq!(paths.protocol_dir(), base.join("data").join("protocol"));
        assert_eq!(paths.sessions_dir(), base.join("data").join("sessions"));
    }

    #[test]
    fn test_storage_paths_all_directories() {
        let paths = StoragePaths::new();

        assert!(paths.tasks_dir().ends_with("tasks"));
        assert!(paths.teammates_dir().ends_with("teammates"));
        assert!(paths.teams_dir().ends_with("teams"));
        assert!(paths.messages_dir().ends_with("messages"));
        assert!(paths.cron_dir().ends_with("cron"));
        assert!(paths.background_dir().ends_with("background"));
        assert!(paths.protocol_dir().ends_with("protocol"));
        assert!(paths.sessions_dir().ends_with("sessions"));
    }

    #[test]
    fn test_storage_paths_file_paths() {
        let paths = StoragePaths::new();

        assert!(paths.task_index().ends_with("tasks/index.json"));
        assert!(paths.teammates_dir().ends_with("teammates"));
        assert!(
            paths
                .teammate_file("alice")
                .ends_with("teammates/alice.json")
        );
        assert!(paths.teammate_index().ends_with("teammates/index.json"));
        assert!(paths.team_file("team-1").ends_with("teams/team-1.json"));
        assert!(paths.team_index().ends_with("teams/index.json"));
        assert!(paths.message_file("bob").ends_with("messages/bob.jsonl"));
        assert!(paths.cron_jobs_file().ends_with("cron/jobs.json"));
        assert!(
            paths
                .cron_runs_file("job-1")
                .ends_with("cron/runs/job-1.jsonl")
        );
        assert!(
            paths
                .background_tasks_file()
                .ends_with("background/tasks.json")
        );
        assert!(
            paths
                .shutdown_requests_file()
                .ends_with("protocol/shutdown_requests.json")
        );
        assert!(
            paths
                .plan_requests_file()
                .ends_with("protocol/plan_requests.json")
        );
        assert!(
            paths
                .session_file("sess-1")
                .ends_with("sessions/sess-1.json")
        );
        assert!(
            paths
                .session_messages_file("sess-1")
                .ends_with("sessions/sess-1_messages.jsonl")
        );
        assert!(paths.sessions_index_file().ends_with("sessions/index.json"));
    }

    #[test]
    fn test_index_operations() {
        let mut index = Index::new();

        let entry1 = IndexEntry {
            id: "task-1".to_string(),
            subject: Some("Task 1".to_string()),
            status: "pending".to_string(),
            updated_at: 1000,
        };

        index.add_entry(entry1);
        assert_eq!(index.items.len(), 1);

        let entry2 = IndexEntry {
            id: "task-2".to_string(),
            subject: Some("Task 2".to_string()),
            status: "in_progress".to_string(),
            updated_at: 2000,
        };

        index.add_entry(entry2);
        assert_eq!(index.items.len(), 2);

        let updated_entry = IndexEntry {
            id: "task-1".to_string(),
            subject: Some("Task 1 Updated".to_string()),
            status: "completed".to_string(),
            updated_at: 3000,
        };

        index.add_entry(updated_entry);
        assert_eq!(index.items.len(), 2);

        let entry = index.get_entry("task-1").unwrap();
        assert_eq!(entry.status, "completed");

        index.remove_entry("task-1");
        assert_eq!(index.items.len(), 1);
        assert!(index.get_entry("task-1").is_none());
    }

    #[test]
    fn test_index_get_entry_missing() {
        let index = Index::new();
        assert!(index.get_entry("nonexistent").is_none());
    }

    #[test]
    fn test_index_remove_missing() {
        let mut index = Index::new();
        index.remove_entry("nonexistent");
        assert!(index.items.is_empty());
    }

    #[test]
    fn test_index_default_values() {
        let index = Index::default();
        assert_eq!(index.version, 1);
        assert!(index.items.is_empty());
        assert!(index.updated_at > 0);
    }

    #[test]
    fn test_index_entry_serialization() {
        let entry = IndexEntry {
            id: "test-id".to_string(),
            subject: Some("Test Subject".to_string()),
            status: "pending".to_string(),
            updated_at: 12345,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: IndexEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, entry.id);
        assert_eq!(deserialized.subject, entry.subject);
        assert_eq!(deserialized.status, entry.status);
        assert_eq!(deserialized.updated_at, entry.updated_at);
    }

    #[test]
    fn test_index_entry_without_subject() {
        let entry = IndexEntry {
            id: "test-id".to_string(),
            subject: None,
            status: "pending".to_string(),
            updated_at: 12345,
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("subject"));

        let deserialized: IndexEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.subject, None);
    }

    #[test]
    fn test_index_serialization() {
        let mut index = Index::new();
        index.add_entry(IndexEntry {
            id: "task-1".to_string(),
            subject: Some("First Task".to_string()),
            status: "completed".to_string(),
            updated_at: 1000,
        });

        let json = serde_json::to_string(&index).unwrap();
        let deserialized: Index = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.items.len(), 1);
        assert_eq!(deserialized.items[0].id, "task-1");
    }

    #[test]
    fn test_file_store_base_path() {
        let base = PathBuf::from("/test/base");
        let store = FileStore::new(base);

        assert_eq!(store.base_path(), Path::new("/test/base"));
    }

    #[test]
    fn test_storage_paths_default() {
        let paths = StoragePaths::default();
        assert!(paths.tasks_dir().ends_with("tasks"));
    }
}
