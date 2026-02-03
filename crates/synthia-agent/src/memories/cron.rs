//! Memory extraction cron job
//!
//! This module implements a cron job that runs the memory extraction pipeline.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use synthia_job::Job;

use super::{phase1, phase2, store::MemoryStore};
use crate::{Result, model_router::ModelRouter, session::SessionManager};

pub struct MemoryExtractionJob {
    memory_store: Arc<dyn MemoryStore>,
    session_store: Arc<dyn SessionManager>,
    model_router: Arc<dyn ModelRouter>,
    workspace: PathBuf,
}

impl MemoryExtractionJob {
    pub fn new(
        memory_store: Arc<dyn MemoryStore>,
        session_store: Arc<dyn SessionManager>,
        model_router: Arc<dyn ModelRouter>,
        workspace: impl Into<PathBuf>,
    ) -> Self {
        Self {
            memory_store,
            session_store,
            model_router,
            workspace: workspace.into(),
        }
    }

    pub async fn run_memory_extraction(&self) -> Result<()> {
        tracing::info!("Starting memory extraction job");

        phase1::run(
            Arc::clone(&self.memory_store),
            Arc::clone(&self.session_store),
            &self.workspace,
        )
        .await?;
        phase2::run(
            Arc::clone(&self.memory_store),
            Arc::clone(&self.model_router),
            &self.workspace,
        )
        .await?;

        tracing::info!("Memory extraction job completed successfully");
        Ok(())
    }
}

#[async_trait]
impl Job for MemoryExtractionJob {
    fn description(&self) -> &str {
        "Memory extraction job"
    }

    fn key(&self) -> &str {
        "memory-extraction"
    }

    async fn execute(&self) {
        if let Err(e) = self.run_memory_extraction().await {
            tracing::error!("Memory extraction job failed: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::memories::MemoryFileStore;

    #[tokio::test]
    async fn test_memory_extraction_job_creation() {
        let temp = tempdir().unwrap();
        let storage: Arc<dyn MemoryStore> =
            Arc::new(MemoryFileStore::with_base(temp.path().to_path_buf()));
        let session: Arc<dyn SessionManager> =
            Arc::new(crate::session::SessionFileStore::new());
        let model_router =
            Arc::new(crate::model_router::FirstModelRouter::default());
        let job = MemoryExtractionJob::new(storage, session, model_router, ".");

        assert_eq!(job.key(), "memory-extraction");
        assert_eq!(job.description(), "Memory extraction job");
    }

    #[tokio::test]
    async fn test_memory_extraction_job_trait() {
        fn assert_job<T: Job>(_job: &T) {}

        let temp = tempdir().unwrap();
        let storage: Arc<dyn MemoryStore> =
            Arc::new(MemoryFileStore::with_base(temp.path().to_path_buf()));
        let session: Arc<dyn SessionManager> =
            Arc::new(crate::session::SessionFileStore::new());
        let model_router =
            Arc::new(crate::model_router::FirstModelRouter::default());
        let job = MemoryExtractionJob::new(storage, session, model_router, ".");
        assert_job(&job);
    }
}
