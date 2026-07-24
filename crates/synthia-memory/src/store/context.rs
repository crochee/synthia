//! Context-memory layer methods on
//! [`super::builder::MemoryStoreImpl`].

use crate::{context::ContextMessage, types::CompactionReport};

impl super::builder::MemoryStoreImpl {
    pub async fn set_context(
        &self,
        session_id: &str,
        messages: Vec<ContextMessage>,
    ) -> Result<(), synthia_core::Error> {
        self.context.set_context(session_id, messages).await
    }

    pub async fn get_context(
        &self,
        session_id: &str,
    ) -> Result<Vec<ContextMessage>, synthia_core::Error> {
        self.context.get_context(session_id).await
    }

    pub async fn compact_context(
        &self,
        session_id: &str,
    ) -> Result<CompactionReport, synthia_core::Error> {
        self.context.compact_context(session_id).await
    }
}
