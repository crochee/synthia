//! Cold-memory layer (JSONL) methods on
//! [`super::builder::MemoryStoreImpl`].

use crate::types::ColdJsonlEntry;

impl super::builder::MemoryStoreImpl {
    pub async fn append_cold(
        &self,
        entry: &ColdJsonlEntry,
    ) -> Result<(), synthia_core::Error> {
        self.cold_jsonl.append(entry).await
    }

    pub async fn append_cold_fields(
        &self,
        summary: &str,
        session_id: &str,
        tools_used: Vec<String>,
        outcome: &str,
    ) -> Result<(), synthia_core::Error> {
        self.cold_jsonl
            .append_cold(summary, session_id, tools_used, outcome)
            .await
    }

    pub async fn search_cold_jsonl(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ColdJsonlEntry>, synthia_core::Error> {
        self.cold_jsonl.search(query, limit).await
    }
}
