use std::path::PathBuf;

use tokio::{fs, io::AsyncWriteExt};

use crate::types::EpisodicJsonlEntry;

const DEFAULT_FILENAME: &str = "episodic.jsonl";

pub struct EpisodicJsonlMemory {
    file_path: PathBuf,
}

impl EpisodicJsonlMemory {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            file_path: base_dir.join(DEFAULT_FILENAME),
        }
    }

    /// Create with an explicit file path (useful for testing).
    pub fn with_path(file_path: PathBuf) -> Self {
        Self { file_path }
    }

    /// Write an EpisodicJsonlEntry to the JSONL file.
    pub async fn write_episodic(
        &self,
        entry: &EpisodicJsonlEntry,
    ) -> Result<(), synthia_core::Error> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let line = serde_json::to_string(entry).map_err(|e| {
            synthia_core::Error::Parse(format!("failed to serialize: {}", e))
        })?;
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .await?
            .write_all(format!("{}\n", line).as_bytes())
            .await?;

        Ok(())
    }

    /// Convenience: create and append an entry from individual fields.
    pub async fn write(
        &self,
        task_hint: &str,
        tools_used: Vec<String>,
        success_count: u64,
        avg_tokens: f64,
    ) -> Result<(), synthia_core::Error> {
        let entry = EpisodicJsonlEntry {
            task_hint: task_hint.to_string(),
            tools_used,
            success_count,
            avg_tokens,
        };
        self.write_episodic(&entry).await
    }

    /// Load all episodic entries from the JSONL file.
    pub async fn load_all(
        &self,
    ) -> Result<Vec<EpisodicJsonlEntry>, synthia_core::Error> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.file_path).await?;
        let entries = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<EpisodicJsonlEntry>(l).ok())
            .collect();

        Ok(entries)
    }

    /// Load episodic entries matching a task hint (substring match).
    pub async fn load_episodic(
        &self,
        task_hint: &str,
    ) -> Result<Vec<EpisodicJsonlEntry>, synthia_core::Error> {
        let entries = self.load_all().await?;
        if entries.is_empty() || task_hint.trim().is_empty() {
            return Ok(Vec::new());
        }

        let hint_lower = task_hint.to_lowercase();
        let results = entries
            .into_iter()
            .filter(|e| e.task_hint.to_lowercase().contains(&hint_lower))
            .collect();

        Ok(results)
    }

    /// Get the file path.
    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = EpisodicJsonlMemory::new(temp_dir.path().to_path_buf());

        let entry = EpisodicJsonlEntry {
            task_hint: "Build REST API".to_string(),
            tools_used: vec!["read".to_string(), "write".to_string()],
            success_count: 5,
            avg_tokens: 1200.0,
        };
        mem.write_episodic(&entry).await.unwrap();

        let entries = mem.load_all().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].task_hint, "Build REST API");
        assert_eq!(entries[0].success_count, 5);
    }

    #[tokio::test]
    async fn test_write_convenience() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = EpisodicJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.write("Python data analysis", vec!["run".to_string()], 3, 800.0)
            .await
            .unwrap();

        let entries = mem.load_all().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].avg_tokens, 800.0);
    }

    #[tokio::test]
    async fn test_load_episodic_by_hint() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = EpisodicJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.write("Build REST API", vec!["read".to_string()], 5, 1200.0)
            .await
            .unwrap();
        mem.write(
            "Build GraphQL endpoint",
            vec!["write".to_string()],
            2,
            900.0,
        )
        .await
        .unwrap();
        mem.write("Python data analysis", vec!["run".to_string()], 3, 800.0)
            .await
            .unwrap();

        let results = mem.load_episodic("Build").await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.task_hint.contains("Build")));
    }

    #[tokio::test]
    async fn test_load_episodic_no_match() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = EpisodicJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.write("Rust async", vec!["read".to_string()], 1, 500.0)
            .await
            .unwrap();

        let results = mem.load_episodic("Python").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_load_empty_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = EpisodicJsonlMemory::new(temp_dir.path().to_path_buf());

        let entries = mem.load_all().await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_load_episodic_empty_hint() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = EpisodicJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.write("Rust async", vec!["read".to_string()], 1, 500.0)
            .await
            .unwrap();

        let results = mem.load_episodic("").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_writes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = EpisodicJsonlMemory::new(temp_dir.path().to_path_buf());

        for i in 0..5 {
            mem.write(
                &format!("Task {}", i),
                vec!["tool".to_string()],
                i,
                100.0 * i as f64,
            )
            .await
            .unwrap();
        }

        let entries = mem.load_all().await.unwrap();
        assert_eq!(entries.len(), 5);
    }
}
