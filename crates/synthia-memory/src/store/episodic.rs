//! Episodic-memory layer (JSONL) methods on
//! [`super::builder::MemoryStoreImpl`].

use crate::types::EpisodicJsonlEntry;

impl super::builder::MemoryStoreImpl {
    pub async fn write_episodic_jsonl(
        &self,
        entry: &EpisodicJsonlEntry,
    ) -> Result<(), synthia_core::Error> {
        self.episodic_jsonl.write_episodic(entry).await
    }

    pub async fn write_episodic_fields(
        &self,
        task_hint: &str,
        tools_used: Vec<String>,
        success_count: u64,
        avg_tokens: f64,
    ) -> Result<(), synthia_core::Error> {
        self.episodic_jsonl
            .write(task_hint, tools_used, success_count, avg_tokens)
            .await
    }

    pub async fn load_episodic_jsonl(
        &self,
        task_hint: &str,
    ) -> Result<Vec<EpisodicJsonlEntry>, synthia_core::Error> {
        self.episodic_jsonl.load_episodic(task_hint).await
    }
}
