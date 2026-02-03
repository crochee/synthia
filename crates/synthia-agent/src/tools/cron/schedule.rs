//! Schedule utilities for cron expression parsing.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::StreamExt;
use rmcp::model::{Role, SamplingContent, SamplingMessageContent};
use synthia_job::{Job, parse_standard};
use tokio_util::sync::CancellationToken;

use super::{data::CronJob, file_store::CronFileStore};
use crate::{Agent, Result, error::AgentError, types::AgentEvent};

pub(super) async fn execute_cron_job(
    job: &CronJob,
    agent: Agent,
) -> (bool, String) {
    execute_agent_job(&job.id, &job.content, agent).await
}

async fn execute_agent_job(
    job_id: &str,
    content: &str,
    agent: Agent,
) -> (bool, String) {
    if content.is_empty() {
        return (false, "Empty prompt".to_string());
    }

    let session = match agent.deps.session.create_session().await {
        Ok(session) => session,
        Err(e) => {
            return (false, format!("Failed to create session: {e}"));
        }
    };
    let mut output_parts = Vec::new();
    output_parts.push(format!("Task started: {job_id}"));
    output_parts.push(format!("session_id: {}", session.id));

    let session_config = session.into();

    let cancel_token = CancellationToken::new();

    let mut success = true;

    let events = agent.react(session_config, cancel_token.clone()).await;

    tokio::pin!(events);

    while let Some(event) = events.next().await {
        match event {
            AgentEvent::Message(msg) => {
                if msg.role == Role::Assistant
                    && let SamplingContent::Single(
                        SamplingMessageContent::Text(text),
                    ) = &msg.content
                {
                    output_parts.push(text.text.clone());
                }
            }
            AgentEvent::Status(status) => {
                match status {
                    crate::types::AgentStatus::Cancelled => {
                        output_parts.push("Task cancelled".to_string());
                        success = false;
                    }
                    crate::types::AgentStatus::Completed => {
                        output_parts
                            .push("Task completed successfully".to_string());
                    }
                    crate::types::AgentStatus::Errored(msg) => {
                        output_parts.push(format!("Task failed: {msg}"));
                        success = false;
                    }
                    _ => {
                        output_parts.push(format!("Task status: {status:?}"));
                    }
                }
                break;
            }
            _ => {}
        }
    }

    let output = if output_parts.is_empty() {
        "Task completed with no output".to_string()
    } else {
        output_parts.join("\n")
    };

    (success, output)
}

/// Wrapper for cron jobs to integrate with the job scheduler
pub struct CronJobWrapper {
    /// The cron job
    pub job: CronJob,
    store: Arc<CronFileStore>,
    agent: Agent,
}

impl std::fmt::Debug for CronJobWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronJobWrapper")
            .field("job", &self.job)
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl CronJobWrapper {
    /// Create a new cron job wrapper
    pub fn new(job: CronJob, store: Arc<CronFileStore>, agent: Agent) -> Self {
        Self { job, store, agent }
    }
}

#[async_trait]
impl Job for CronJobWrapper {
    fn description(&self) -> &str {
        &self.job.description
    }

    fn key(&self) -> &str {
        &self.job.id
    }

    async fn execute(&self) {
        let started_at = Utc::now();
        let (success, output) = execute_agent_job(
            &self.job.id,
            &self.job.content,
            self.agent.clone(),
        )
        .await;
        let finished_at = Utc::now();
        let duration_ms = (finished_at - started_at).num_milliseconds();
        let status = if success { "ok" } else { "error" };

        if let Err(e) = self
            .store
            .record_run(
                &self.job.id,
                started_at,
                finished_at,
                status,
                Some(&output),
                duration_ms,
            )
            .await
        {
            tracing::error!(
                "Failed to record run for job {}: {}",
                self.job.id,
                e
            );
            return;
        }

        if let Ok(next) = next_run(&self.job.crontab, Utc::now())
            && let Err(e) = self
                .store
                .update_job_after_run(
                    &self.job.id,
                    next,
                    finished_at,
                    status,
                    Some(&output),
                )
                .await
        {
            tracing::error!("Failed to update job {}: {}", self.job.id, e);
        }
    }
}

pub(crate) fn next_run(
    crontab: &str,
    from: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let trigger = parse_standard(crontab).map_err(|e| {
        AgentError::ValidationError(format!("Invalid cron expression: {e}"))
    })?;
    let from_ns = from.timestamp_nanos_opt().ok_or_else(|| {
        AgentError::ValidationError("Invalid timestamp".to_string())
    })?;
    trigger
        .next_fire_time(from_ns)
        .map(|ns| Utc.timestamp_nanos(ns))
        .ok_or_else(|| {
            AgentError::ValidationError(
                "No future runs found for cron expression".to_string(),
            )
        })
}
