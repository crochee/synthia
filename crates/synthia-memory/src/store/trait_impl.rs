//! The [`crate::types::MemoryStore`] trait impl for
//! [`super::builder::MemoryStoreImpl`], plus the 2
//! private conversion helpers
//! ([`cold_jsonl_to_cold`] +
//! [`episodic_jsonl_to_skill`]).

use async_trait::async_trait;

use super::builder::MemoryStoreImpl;
use crate::{
    retrieval,
    types::{
        ColdEntry,
        ColdJsonlEntry,
        CompactionReport,
        EpisodicJsonlEntry,
        EpisodicSkill,
        RetrievalMode,
        SearchResult,
    },
};

/// Bridge ColdJsonlEntry to ColdEntry using
/// ColdEntry::new_jsonl().
fn cold_jsonl_to_cold(entry: &ColdJsonlEntry) -> ColdEntry {
    ColdEntry::new_jsonl(
        entry.session_id.clone(),
        entry.timestamp,
        entry.summary.clone(),
        entry.session_id.clone(),
        entry.tools_used.clone(),
        entry.outcome.clone(),
    )
}

/// Bridge EpisodicJsonlEntry to EpisodicSkill for the
/// MemoryStore trait.
fn episodic_jsonl_to_skill(entry: &EpisodicJsonlEntry) -> EpisodicSkill {
    let total_count = entry.tools_used.len().max(1) as u64;
    EpisodicSkill {
        task_hint: entry.task_hint.clone(),
        skill_content: serde_json::to_string(entry).unwrap_or_default(),
        success_rate: entry.success_count as f64 / total_count as f64,
        used_at: chrono::Utc::now(),
    }
}

#[async_trait]
impl crate::types::MemoryStore for MemoryStoreImpl {
    async fn write_hot(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), synthia_core::Error> {
        self.hot.write(key, value).await
    }

    async fn read_hot(
        &self,
        key: &str,
    ) -> Result<Option<String>, synthia_core::Error> {
        self.hot.read(key).await
    }

    async fn search_cold(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ColdEntry>, synthia_core::Error> {
        let entries = self.cold_jsonl.search(query, limit).await?;
        Ok(entries.iter().map(cold_jsonl_to_cold).collect())
    }

    async fn search_cold_with_mode(
        &self,
        query: &str,
        limit: usize,
        mode: RetrievalMode,
    ) -> Result<Vec<SearchResult>, synthia_core::Error> {
        // If SQLite-backed cold memory is available, delegate to it.
        if let Some(ref cold_sqlite) = self.cold_sqlite {
            return cold_sqlite.search_with_mode(query, limit, mode).await;
        }

        // Fallback: use semantic_search (keyword matching) on JSONL entries.
        let entries = self.cold_jsonl.search(query, usize::MAX).await?;
        let cold_entries: Vec<ColdEntry> =
            entries.iter().map(cold_jsonl_to_cold).collect();
        Ok(retrieval::semantic_search(&cold_entries, query, limit))
    }

    async fn write_cold(
        &self,
        entry: ColdEntry,
    ) -> Result<(), synthia_core::Error> {
        let jsonl_entry = ColdJsonlEntry {
            timestamp: entry.created_at,
            summary: entry.content.clone(),
            session_id: entry.id.clone(),
            tools_used: entry
                .metadata
                .get("tools_used")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            outcome: entry
                .metadata
                .get("outcome")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
        };
        self.cold_jsonl.append(&jsonl_entry).await
    }

    async fn load_episodic(
        &self,
        task_hint: &str,
    ) -> Result<Vec<EpisodicSkill>, synthia_core::Error> {
        let entries = self.episodic_jsonl.load_episodic(task_hint).await?;
        Ok(entries.iter().map(episodic_jsonl_to_skill).collect())
    }

    async fn save_episodic(
        &self,
        skill: EpisodicSkill,
    ) -> Result<(), synthia_core::Error> {
        let jsonl_entry = EpisodicJsonlEntry {
            task_hint: skill.task_hint,
            tools_used: vec![],
            success_count: (skill.success_rate * 10.0) as u64,
            avg_tokens: skill.skill_content.len() as f64,
        };
        self.episodic_jsonl.write_episodic(&jsonl_entry).await
    }

    async fn compact_context(
        &self,
        session_id: &str,
    ) -> Result<CompactionReport, synthia_core::Error> {
        self.context.compact_context(session_id).await
    }
}
