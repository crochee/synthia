use std::path::PathBuf;

use chrono::Utc;
use tokio::{fs, io::AsyncWriteExt};

use crate::types::ColdJsonlEntry;

const DEFAULT_FILENAME: &str = "cold.jsonl";

pub struct ColdJsonlMemory {
    file_path: PathBuf,
}

impl ColdJsonlMemory {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            file_path: base_dir.join(DEFAULT_FILENAME),
        }
    }

    /// Create with an explicit file path (useful for testing).
    pub fn with_path(file_path: PathBuf) -> Self {
        Self { file_path }
    }

    /// Append a ColdJsonlEntry to the JSONL file.
    pub async fn append(
        &self,
        entry: &ColdJsonlEntry,
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
    pub async fn append_cold(
        &self,
        summary: &str,
        session_id: &str,
        tools_used: Vec<String>,
        outcome: &str,
    ) -> Result<(), synthia_core::Error> {
        let entry = ColdJsonlEntry {
            timestamp: Utc::now(),
            summary: summary.to_string(),
            session_id: session_id.to_string(),
            tools_used,
            outcome: outcome.to_string(),
        };
        self.append(&entry).await
    }

    /// Read all entries from the JSONL file.
    pub async fn load_all(
        &self,
    ) -> Result<Vec<ColdJsonlEntry>, synthia_core::Error> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.file_path).await?;
        let entries = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<ColdJsonlEntry>(l).ok())
            .collect();

        Ok(entries)
    }

    /// Search cold memory by keyword matching on summary and outcome fields.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ColdJsonlEntry>, synthia_core::Error> {
        let entries = self.load_all().await?;
        if entries.is_empty() || query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<ColdJsonlEntry> = entries
            .into_iter()
            .filter(|e| {
                let summary_lower = e.summary.to_lowercase();
                let outcome_lower = e.outcome.to_lowercase();
                let tools_lower: Vec<String> =
                    e.tools_used.iter().map(|t| t.to_lowercase()).collect();

                // Match if any query term appears in summary, outcome, or tools_used
                query_terms.iter().any(|term| {
                    summary_lower.contains(term)
                        || outcome_lower.contains(term)
                        || tools_lower.iter().any(|t| t.contains(term))
                })
            })
            .collect();

        results.truncate(limit);
        Ok(results)
    }

    /// Get the file path.
    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[tokio::test]
    async fn test_append_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = ColdJsonlMemory::new(temp_dir.path().to_path_buf());

        let entry = ColdJsonlEntry {
            timestamp: Utc::now(),
            summary: "Built REST API".to_string(),
            session_id: "sess-1".to_string(),
            tools_used: vec!["read".to_string(), "write".to_string()],
            outcome: "success".to_string(),
        };
        mem.append(&entry).await.unwrap();

        let entries = mem.load_all().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].summary, "Built REST API");
    }

    #[tokio::test]
    async fn test_append_cold_convenience() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = ColdJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.append_cold(
            "Refactored auth module",
            "sess-2",
            vec!["search".to_string(), "edit".to_string()],
            "completed",
        )
        .await
        .unwrap();

        let entries = mem.load_all().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id, "sess-2");
    }

    #[tokio::test]
    async fn test_load_empty_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = ColdJsonlMemory::new(temp_dir.path().to_path_buf());

        let entries = mem.load_all().await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_search_by_keyword() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = ColdJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.append_cold(
            "Rust API development",
            "sess-1",
            vec!["read".to_string()],
            "success",
        )
        .await
        .unwrap();
        mem.append_cold(
            "Python data analysis",
            "sess-2",
            vec!["run".to_string()],
            "success",
        )
        .await
        .unwrap();
        mem.append_cold(
            "Rust async patterns",
            "sess-3",
            vec!["write".to_string()],
            "partial",
        )
        .await
        .unwrap();

        let results = mem.search("Rust", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.summary.contains("Rust")));
    }

    #[tokio::test]
    async fn test_search_respects_limit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = ColdJsonlMemory::new(temp_dir.path().to_path_buf());

        for i in 0..10 {
            mem.append_cold(
                &format!("Rust task {}", i),
                &format!("sess-{}", i),
                vec!["tool".to_string()],
                "success",
            )
            .await
            .unwrap();
        }

        let results = mem.search("Rust", 3).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_search_empty_query() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = ColdJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.append_cold(
            "Rust task",
            "sess-1",
            vec!["read".to_string()],
            "success",
        )
        .await
        .unwrap();

        let results = mem.search("", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_appends() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mem = ColdJsonlMemory::new(temp_dir.path().to_path_buf());

        mem.append_cold("Entry one", "s1", vec![], "ok")
            .await
            .unwrap();
        mem.append_cold("Entry two", "s2", vec![], "ok")
            .await
            .unwrap();
        mem.append_cold("Entry three", "s3", vec![], "ok")
            .await
            .unwrap();

        let entries = mem.load_all().await.unwrap();
        assert_eq!(entries.len(), 3);
    }
}
