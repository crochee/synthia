//! Maintenance operations on [`super::core::ColdMemory`].

use std::path::{Path, PathBuf};

use chrono::Utc;

use super::core::ColdMemory;
use crate::types::ColdEntry;

impl ColdMemory {
    /// Total number of entries in cold memory (counts FTS table).
    pub async fn entry_count(&self) -> Result<usize, synthia_core::Error> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM cold_entries_fts")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    synthia_core::Error::Io(std::io::Error::other(format!(
                        "Failed to count entries: {}",
                        e
                    )))
                })?;
        Ok(count.0 as usize)
    }

    /// Delete entries from cold memory by their IDs. Empty input
    /// is a no-op. Returns the number of entries actually deleted.
    pub async fn delete_entries(
        &self,
        ids: &[&str],
    ) -> Result<usize, synthia_core::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut deleted = 0;
        for id in ids {
            sqlx::query("DELETE FROM cold_entries_fts WHERE entry_id = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    synthia_core::Error::Io(std::io::Error::other(format!(
                        "Failed to delete FTS entry: {}",
                        e
                    )))
                })?;
            sqlx::query("DELETE FROM cold_entries_meta WHERE entry_id = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    synthia_core::Error::Io(std::io::Error::other(format!(
                        "Failed to delete meta entry: {}",
                        e
                    )))
                })?;
            deleted += 1;
        }
        Ok(deleted)
    }

    /// Load all cold entries from the database, ordered by
    /// `created_at` DESC.
    pub(super) async fn load_all_entries(
        &self,
    ) -> Result<Vec<ColdEntry>, synthia_core::Error> {
        let rows: Vec<(String, String, String, String, f64, i64)> = sqlx::query_as(
            "SELECT f.entry_id, f.content, m.metadata, m.created_at, m.importance_score, m.access_count FROM cold_entries_fts f JOIN cold_entries_meta m ON m.entry_id = f.entry_id ORDER BY m.created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| synthia_core::Error::Io(std::io::Error::other(format!(
            "Failed to load cold entries: {}",
            e
        ))))?;

        let entries: Vec<ColdEntry> = rows
            .into_iter()
            .map(
                |(
                    id,
                    content,
                    metadata,
                    created_at_str,
                    importance_score,
                    access_count,
                )| {
                    let parsed_metadata: serde_json::Value =
                        serde_json::from_str(&metadata)
                            .unwrap_or(serde_json::Value::Null);
                    let parsed_created_at =
                        chrono::DateTime::parse_from_rfc3339(&created_at_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                    ColdEntry {
                        id,
                        content,
                        metadata: parsed_metadata,
                        created_at: parsed_created_at,
                        importance_score,
                        access_count: access_count as u64,
                        ..Default::default()
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    /// Flush cold memory entries to a markdown file at
    /// `base_dir/memory/YYYY-MM-DD.md`. Returns the path to the
    /// flushed file. Errors if there are no entries to flush.
    pub async fn flush_to_file(
        &self,
        base_dir: &Path,
    ) -> Result<PathBuf, synthia_core::Error> {
        let entries = self.load_all_entries().await?;
        if entries.is_empty() {
            return Err(synthia_core::Error::Internal(
                "No entries to flush".to_string(),
            ));
        }

        let memory_dir = base_dir.join("memory");
        tokio::fs::create_dir_all(&memory_dir).await.map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to create memory dir: {}",
                e
            )))
        })?;

        let filename = format!("{}.md", Utc::now().format("%Y-%m-%d"));
        let file_path = memory_dir.join(&filename);

        let mut content = String::new();
        content.push_str(&format!(
            "# Memory Flush - {}\n\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        ));
        content.push_str(&format!("Total entries: {}\n\n", entries.len()));

        for entry in &entries {
            content.push_str(&format!("## Entry: {}\n\n", entry.id));
            content.push_str(&format!("{}\n\n", entry.content));
        }

        tokio::fs::write(&file_path, content).await.map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to write memory file: {}",
                e
            )))
        })?;

        Ok(file_path)
    }
}
