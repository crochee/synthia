//! Cron Add Tool
//!
//! This tool allows creating new scheduled cron jobs.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;
use synthia_job::{TimeWheel, parse_standard};

use super::{data::CronJob, file_store::CronFileStore, types::CronAddRequest};
use crate::{Agent, tools::Tool};

pub(crate) struct CronAddTool {
    store: Arc<CronFileStore>,
    time_wheel: Arc<TimeWheel>,
    agent: Agent,
}

impl std::fmt::Debug for CronAddTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronAddTool")
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl Clone for CronAddTool {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            time_wheel: Arc::clone(&self.time_wheel),
            agent: self.agent.clone(),
        }
    }
}

impl CronAddTool {
    pub(crate) fn new(
        store: Arc<CronFileStore>,
        time_wheel: Arc<TimeWheel>,
        agent: Agent,
    ) -> Self {
        Self {
            store,
            time_wheel,
            agent,
        }
    }
}

#[async_trait]
impl Tool for CronAddTool {
    fn name(&self) -> &str {
        "cron_add"
    }

    fn description(&self) -> &str {
        "Create scheduled cron job. Supports cron expressions and descriptors."
    }

    fn parameters(&self) -> Value {
        let schema = schemars::schema_for!(CronAddRequest);
        serde_json::to_value(schema).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: CronAddRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let trigger = match parse_standard(&request.crontab) {
            Ok(t) => t,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid crontab: {e}"
                ))]);
            }
        };

        let now = chrono::Utc::now();
        let next = if request.enabled.unwrap_or(true) {
            Some(
                super::schedule::next_run(&request.crontab, now)
                    .unwrap_or_default(),
            )
        } else {
            None
        };

        let job = CronJob {
            id: uuid::Uuid::new_v4().to_string(),
            crontab: request.crontab,
            description: request.description,
            content: request.content,
            enabled: request.enabled.unwrap_or(true),
            created_at: now,
            next_run: next,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        if let Err(e) = self.store.add_job(&job).await {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to add job: {e}"
            ))]);
        }

        if job.enabled {
            let job_wrapper = Arc::new(super::schedule::CronJobWrapper::new(
                job.clone(),
                Arc::clone(&self.store),
                self.agent.clone(),
            ));

            if let Err(e) = self
                .time_wheel
                .schedule_async(job_wrapper, Arc::from(trigger))
                .await
            {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to schedule job: {e}"
                ))]);
            }
        }

        let result = serde_json::to_value(&job).unwrap_or_default();
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        CallToolResult::success(vec![Content::text(json)])
    }
}
