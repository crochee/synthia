use std::path::PathBuf;

use async_trait::async_trait;

use super::{Memory, MemoryImportance, MemoryQuery, MemoryStats, Stage1Output};
use crate::{AgentError, Result};

#[derive(Debug, Clone)]
pub struct MemoryFileStore {
    base_path: PathBuf,
}

impl MemoryFileStore {
    pub fn new() -> Self {
        let base_path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".agent")
            .join("memories");
        Self { base_path }
    }

    pub fn with_base(base: PathBuf) -> Self {
        let base_path = base.join("memories");
        Self { base_path }
    }

    async fn ensure_dir(&self, path: &std::path::Path) -> Result<()> {
        if !path.exists() {
            tokio::fs::create_dir_all(path).await.map_err(|e| {
                AgentError::internal(format!(
                    "Failed to create directory '{}': {}",
                    path.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    fn memories_file(&self) -> PathBuf {
        self.base_path.join("memories.json")
    }

    fn stage1_outputs_dir(&self) -> PathBuf {
        self.base_path.join("stage1_outputs")
    }

    fn stage1_output_file(&self, thread_id: &str) -> PathBuf {
        self.stage1_outputs_dir().join(format!("{thread_id}.json"))
    }

    fn consolidated_memories_file(&self) -> PathBuf {
        self.base_path.join("consolidated_memories.json")
    }

    async fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &PathBuf,
    ) -> Result<T> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to read file '{}': {}",
                path.display(),
                e
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

    async fn write_json<T: serde::Serialize + Sync>(
        &self,
        path: &PathBuf,
        data: &T,
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            self.ensure_dir(parent).await?;
        }
        let content = serde_json::to_string_pretty(data).map_err(|e| {
            AgentError::internal(format!("Failed to serialize JSON: {e}"))
        })?;
        tokio::fs::write(path, content).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to write file '{}': {}",
                path.display(),
                e
            ))
        })?;
        Ok(())
    }

    async fn load_memories(&self) -> Result<Vec<Memory>> {
        let path = self.memories_file();
        if !path.exists() {
            return Ok(Vec::new());
        }
        self.read_json(&path).await
    }

    async fn save_memories(&self, memories: &[Memory]) -> Result<()> {
        let path = self.memories_file();
        self.write_json(&path, &memories).await
    }

    async fn load_stage1_outputs(&self) -> Result<Vec<Stage1Output>> {
        let dir = self.stage1_outputs_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries = tokio::fs::read_dir(&dir).await.map_err(|e| {
            AgentError::internal(format!(
                "Failed to read directory '{}': {}",
                dir.display(),
                e
            ))
        })?;
        let mut outputs = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AgentError::internal(
                format!("Failed to read directory entry: {e}",),
            )
        })? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Ok(output) = self.read_json::<Stage1Output>(&path).await
            {
                outputs.push(output);
            }
        }
        Ok(outputs)
    }
}

impl Default for MemoryFileStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn store(&self, memory: &Memory) -> Result<()>;
    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<Memory>>;
    async fn get_by_session(&self, session_id: &str) -> Result<Vec<Memory>>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn update_access(&self, id: &str) -> Result<(), AgentError>;
    async fn get_stats(&self) -> Result<MemoryStats>;
    async fn clean_old_memories(&self, days: i64) -> Result<usize>;
    async fn get_recent_memories(&self, limit: usize) -> Result<Vec<Memory>>;
    async fn get_recent_stage1_outputs(
        &self,
        limit: usize,
    ) -> Result<Vec<Stage1Output>>;
    async fn store_stage1_output(&self, output: &Stage1Output) -> Result<()>;
    async fn store_consolidated_memory(
        &self,
        topic: &str,
        content: &str,
    ) -> Result<()>;
}

#[async_trait]
impl MemoryStore for MemoryFileStore {
    async fn store(&self, memory: &Memory) -> Result<()> {
        self.ensure_dir(&self.base_path).await?;
        let mut memories = self.load_memories().await?;

        if let Some(pos) = memories.iter().position(|m| m.id == memory.id) {
            memories[pos] = memory.clone();
        } else {
            memories.push(memory.clone());
        }

        self.save_memories(&memories).await
    }

    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<Memory>> {
        let memories = self.load_memories().await?;

        let filtered: Vec<Memory> = memories
            .into_iter()
            .filter(|m| {
                if let Some(ref session_id) = query.session_id
                    && &m.session_id != session_id
                {
                    return false;
                }

                if let Some(ref types) = query.memory_types
                    && !types.is_empty()
                    && !types.contains(&m.memory_type)
                {
                    return false;
                }

                if let Some(ref min_importance) = query.min_importance
                    && m.importance.score() < min_importance.score()
                {
                    return false;
                }

                if let Some(ref tags) = query.tags
                    && !tags.is_empty()
                    && !tags.iter().any(|t| m.tags.contains(t))
                {
                    return false;
                }

                true
            })
            .collect();

        let mut sorted: Vec<_> = filtered;
        sorted.sort_by(|a, b| b.accessed_at.cmp(&a.accessed_at));

        if query.limit > 0 && sorted.len() > query.limit {
            sorted.truncate(query.limit);
        }

        Ok(sorted)
    }

    async fn get_by_session(&self, session_id: &str) -> Result<Vec<Memory>> {
        let query = MemoryQuery {
            session_id: Some(session_id.to_string()),
            ..Default::default()
        };
        self.recall(&query).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut memories = self.load_memories().await?;
        memories.retain(|m| m.id != id);
        self.save_memories(&memories).await
    }

    async fn update_access(&self, id: &str) -> Result<(), crate::AgentError> {
        let mut memories = self.load_memories().await?;
        if let Some(memory) = memories.iter_mut().find(|m| m.id == id) {
            memory.bump_access();
            self.save_memories(&memories).await?;
        }
        Ok(())
    }

    async fn get_stats(&self) -> Result<MemoryStats> {
        let memories = self.load_memories().await?;

        let total_memories = memories.len();

        let mut by_type: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut by_importance: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut total_access: u64 = 0;

        for m in &memories {
            *by_type
                .entry(m.memory_type.as_str().to_string())
                .or_insert(0) += 1;
            *by_importance
                .entry(m.importance.as_str().to_string())
                .or_insert(0) += 1;
            total_access += m.access_count as u64;
        }

        let avg_access_count = if total_memories > 0 {
            total_access as f32 / total_memories as f32
        } else {
            0.0
        };

        Ok(MemoryStats {
            total_memories,
            by_type,
            by_importance,
            avg_access_count,
        })
    }

    async fn clean_old_memories(&self, days: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let mut memories = self.load_memories().await?;

        let original_len = memories.len();
        memories.retain(|m| {
            m.importance == MemoryImportance::Critical || m.accessed_at > cutoff
        });

        let cleaned = original_len - memories.len();
        self.save_memories(&memories).await?;
        Ok(cleaned)
    }

    async fn get_recent_memories(&self, limit: usize) -> Result<Vec<Memory>> {
        let query = MemoryQuery {
            limit,
            ..Default::default()
        };
        self.recall(&query).await
    }

    async fn get_recent_stage1_outputs(
        &self,
        limit: usize,
    ) -> Result<Vec<Stage1Output>> {
        let outputs = self.load_stage1_outputs().await?;
        let mut sorted = outputs;
        sorted.sort_by(|a, b| b.source_updated_at.cmp(&a.source_updated_at));
        if limit > 0 && sorted.len() > limit {
            sorted.truncate(limit);
        }
        Ok(sorted)
    }

    async fn store_stage1_output(&self, output: &Stage1Output) -> Result<()> {
        let path = self.stage1_output_file(&output.thread_id);
        self.write_json(&path, output).await
    }

    async fn store_consolidated_memory(
        &self,
        topic: &str,
        content: &str,
    ) -> Result<()> {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct ConsolidatedMemory {
            topic: String,
            content: String,
            created_at: i64,
        }

        let path = self.consolidated_memories_file();
        let mut memories: Vec<ConsolidatedMemory> = if path.exists() {
            self.read_json(&path).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        if let Some(existing) = memories.iter_mut().find(|m| m.topic == topic) {
            existing.content = content.to_string();
            existing.created_at = chrono::Utc::now().timestamp();
        } else {
            memories.push(ConsolidatedMemory {
                topic: topic.to_string(),
                content: content.to_string(),
                created_at: chrono::Utc::now().timestamp(),
            });
        }

        self.write_json(&path, &memories).await
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::memories::{MemoryImportance, MemoryType};

    #[tokio::test]
    async fn test_memory_store_basic() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let memory = Memory::new(
            "session-1",
            "Test memory".to_string(),
            MemoryType::KeyInsight,
        );

        store.store(&memory).await.unwrap();

        let recalled = store.recall(&MemoryQuery::default()).await.unwrap();
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, "Test memory");
    }

    #[tokio::test]
    async fn test_memory_store_by_session() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let m1 = Memory::new(
            "session-1",
            "Memory 1".to_string(),
            MemoryType::KeyInsight,
        );
        let m2 = Memory::new(
            "session-2",
            "Memory 2".to_string(),
            MemoryType::KeyInsight,
        );

        store.store(&m1).await.unwrap();
        store.store(&m2).await.unwrap();

        let session1_memories =
            store.get_by_session("session-1").await.unwrap();
        assert_eq!(session1_memories.len(), 1);
        assert_eq!(session1_memories[0].content, "Memory 1");
    }

    #[tokio::test]
    async fn test_memory_store_delete() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let memory = Memory::new(
            "session-1",
            "To delete".to_string(),
            MemoryType::KeyInsight,
        );
        store.store(&memory).await.unwrap();

        store.delete(&memory.id).await.unwrap();

        let recalled = store.recall(&MemoryQuery::default()).await.unwrap();
        assert!(recalled.is_empty());
    }

    #[tokio::test]
    async fn test_memory_store_stats() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let m1 = Memory::new(
            "session-1",
            "Memory 1".to_string(),
            MemoryType::KeyInsight,
        );
        let m2 = Memory::new(
            "session-2",
            "Memory 2".to_string(),
            MemoryType::UserPreference,
        );

        store.store(&m1).await.unwrap();
        store.store(&m2).await.unwrap();

        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.total_memories, 2);
        assert_eq!(stats.by_type.get("key_insight"), Some(&1));
        assert_eq!(stats.by_type.get("user_preference"), Some(&1));
    }

    #[tokio::test]
    async fn test_stage1_outputs() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let output = Stage1Output {
            thread_id: "thread-1".to_string(),
            raw_memory: "raw".to_string(),
            rollout_summary: "summary".to_string(),
            cwd: temp.path().to_path_buf(),
            source_updated_at: chrono::Utc::now(),
        };

        store.store_stage1_output(&output).await.unwrap();

        let outputs = store.get_recent_stage1_outputs(10).await.unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].thread_id, "thread-1");
    }

    #[tokio::test]
    async fn test_recall_filter_by_memory_types() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let m1 =
            Memory::new("s1", "Insight".to_string(), MemoryType::KeyInsight);
        let m2 = Memory::new(
            "s2",
            "Preference".to_string(),
            MemoryType::UserPreference,
        );

        store.store(&m1).await.unwrap();
        store.store(&m2).await.unwrap();

        let query = MemoryQuery {
            memory_types: Some(vec![MemoryType::KeyInsight]),
            ..Default::default()
        };
        let results = store.recall(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Insight");
    }

    #[tokio::test]
    async fn test_recall_filter_by_min_importance() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let m1 = Memory::new("s1", "Low".to_string(), MemoryType::KeyInsight)
            .with_importance(MemoryImportance::Low);
        let m2 = Memory::new("s2", "High".to_string(), MemoryType::KeyInsight)
            .with_importance(MemoryImportance::High);

        store.store(&m1).await.unwrap();
        store.store(&m2).await.unwrap();

        let query = MemoryQuery {
            min_importance: Some(MemoryImportance::High),
            ..Default::default()
        };
        let results = store.recall(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "High");
    }

    #[tokio::test]
    async fn test_recall_filter_by_tags() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let m1 =
            Memory::new("s1", "With tag".to_string(), MemoryType::KeyInsight)
                .with_tags(vec!["rust".to_string()]);
        let m2 =
            Memory::new("s2", "No tag".to_string(), MemoryType::KeyInsight);

        store.store(&m1).await.unwrap();
        store.store(&m2).await.unwrap();

        let query = MemoryQuery {
            tags: Some(vec!["rust".to_string()]),
            ..Default::default()
        };
        let results = store.recall(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "With tag");
    }

    #[tokio::test]
    async fn test_recall_limit() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        for i in 0..5 {
            let m = Memory::new(
                "s1",
                format!("Memory {i}"),
                MemoryType::KeyInsight,
            );
            store.store(&m).await.unwrap();
        }

        let query = MemoryQuery {
            limit: 3,
            ..Default::default()
        };
        let results = store.recall(&query).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_update_access() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let mut memory =
            Memory::new("s1", "Test".to_string(), MemoryType::KeyInsight);
        let original_count = memory.access_count;
        store.store(&memory).await.unwrap();

        memory.bump_access();
        store.update_access(&memory.id).await.unwrap();

        let recalled = store.recall(&MemoryQuery::default()).await.unwrap();
        assert_eq!(recalled[0].access_count, original_count + 1);
    }

    #[tokio::test]
    async fn test_clean_old_memories_preserves_critical() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let critical =
            Memory::new("s1", "Critical".to_string(), MemoryType::KeyInsight)
                .with_importance(MemoryImportance::Critical);

        store.store(&critical).await.unwrap();
        let cleaned = store.clean_old_memories(0).await.unwrap();
        assert_eq!(cleaned, 0);

        let remaining = store.recall(&MemoryQuery::default()).await.unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[tokio::test]
    async fn test_store_consolidated_memory() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        store
            .store_consolidated_memory("topic1", "content1")
            .await
            .unwrap();

        let path = temp
            .path()
            .join("memories")
            .join("consolidated_memories.json");
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("topic1"));
        assert!(content.contains("content1"));
    }

    #[tokio::test]
    async fn test_store_consolidated_memory_updates_existing() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        store
            .store_consolidated_memory("topic1", "original")
            .await
            .unwrap();
        store
            .store_consolidated_memory("topic1", "updated")
            .await
            .unwrap();

        let path = temp
            .path()
            .join("memories")
            .join("consolidated_memories.json");
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("updated"));
        assert!(!content.contains("original"));
    }

    #[tokio::test]
    async fn test_get_recent_memories() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let m1 = Memory::new("s1", "First".to_string(), MemoryType::KeyInsight);
        let m2 =
            Memory::new("s1", "Second".to_string(), MemoryType::KeyInsight);

        store.store(&m1).await.unwrap();
        store.store(&m2).await.unwrap();

        let recent = store.get_recent_memories(1).await.unwrap();
        assert_eq!(recent.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_file_store_new_uses_default_path() {
        let store = MemoryFileStore::new();
        let expected_base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".agent")
            .join("memories");
        assert_eq!(store.base_path, expected_base);
    }

    #[tokio::test]
    async fn test_stage1_outputs_sorted_by_source_updated_at() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let old = Stage1Output {
            thread_id: "old".to_string(),
            raw_memory: "old".to_string(),
            rollout_summary: "old".to_string(),
            cwd: temp.path().to_path_buf(),
            source_updated_at: chrono::Utc::now() - chrono::Duration::hours(1),
        };
        let recent = Stage1Output {
            thread_id: "recent".to_string(),
            raw_memory: "recent".to_string(),
            rollout_summary: "recent".to_string(),
            cwd: temp.path().to_path_buf(),
            source_updated_at: chrono::Utc::now(),
        };

        store.store_stage1_output(&old).await.unwrap();
        store.store_stage1_output(&recent).await.unwrap();

        let outputs = store.get_recent_stage1_outputs(10).await.unwrap();
        assert_eq!(outputs[0].thread_id, "recent");
    }

    #[tokio::test]
    async fn test_store_memory_updates_existing() {
        let temp = tempdir().unwrap();
        let store = MemoryFileStore::with_base(temp.path().to_path_buf());

        let mut m1 =
            Memory::new("s1", "Original".to_string(), MemoryType::KeyInsight);
        let _id = m1.id.clone();
        store.store(&m1).await.unwrap();

        m1.content = "Updated".to_string();
        store.store(&m1).await.unwrap();

        let recalled = store.recall(&MemoryQuery::default()).await.unwrap();
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, "Updated");
    }
}
