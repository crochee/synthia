//! Job scheduler module
//!
//! Manages scheduled jobs and memory extraction tasks.

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use synthia_agent::{
    Agent,
    memories::{MemoryFileStore, MemoryStore, cron::MemoryExtractionJob},
    session::{SessionFileStore, SessionManager},
    tools::{CronFileStore, CronJobWrapper},
};
use synthia_job::{TimeWheel, parse_standard};
use tokio_util::sync::CancellationToken;

pub async fn run(
    agent: &Agent,
    workspace: &Path,
    cancel_token: CancellationToken,
) -> Result<()> {
    let memory_store: Arc<dyn MemoryStore> = Arc::new(MemoryFileStore::new());
    let session_manager: Arc<dyn SessionManager> =
        Arc::new(SessionFileStore::new());
    let cron_store = Arc::new(CronFileStore::new());
    let time_wheel = Arc::new(TimeWheel::new());

    if let Err(e) = load_and_schedule_jobs(
        &cron_store,
        &time_wheel,
        agent.clone(),
        workspace,
    )
    .await
    {
        tracing::error!("Failed to load and schedule jobs: {}", e);
    }

    if let Err(e) = schedule_memory_extraction(
        memory_store,
        session_manager,
        agent.clone(),
        workspace,
        &time_wheel,
    )
    .await
    {
        tracing::error!("Failed to schedule memory extraction: {}", e);
    }

    if let Err(e) = time_wheel.run(cancel_token).await {
        tracing::error!("TimeWheel run error: {}", e);
    }

    Ok(())
}

async fn load_and_schedule_jobs(
    store: &Arc<CronFileStore>,
    wheel: &Arc<TimeWheel>,
    agent: Agent,
    _workspace: &Path,
) -> Result<()> {
    let jobs = store.list_jobs().await?;

    for job in jobs {
        if !job.enabled {
            continue;
        }

        let job_wrapper =
            Arc::new(CronJobWrapper::new(job, store.clone(), agent.clone()));

        let trigger: Box<dyn synthia_job::Trigger> =
            match parse_standard(&job_wrapper.job.crontab) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse crontab '{}': {}",
                        job_wrapper.job.crontab,
                        e
                    );
                    continue;
                }
            };

        wheel
            .schedule_async(job_wrapper, Arc::from(trigger))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to schedule job: {}", e))?;
    }

    Ok(())
}

async fn schedule_memory_extraction(
    memory_store: Arc<dyn MemoryStore>,
    session_store: Arc<dyn SessionManager>,
    agent: Agent,
    workspace: &Path,
    wheel: &Arc<TimeWheel>,
) -> Result<()> {
    let memory_job = Arc::new(MemoryExtractionJob::new(
        memory_store,
        session_store,
        agent.deps.router.clone(),
        workspace,
    ));

    let memory_trigger = parse_standard("0 */30 * * * *")
        .context("Failed to parse memory extraction schedule")?;

    wheel
        .schedule_async(memory_job, Arc::from(memory_trigger))
        .await
        .map_err(|e| {
            anyhow::anyhow!("Failed to schedule memory extraction job: {}", e)
        })?;

    Ok(())
}
