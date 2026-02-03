//! Cron Update Tool
//!
//! This tool allows updating an existing scheduled cron job.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;
use synthia_job::{TimeWheel, parse_standard};

use super::{
    data::CronJobPatch,
    file_store::CronFileStore,
    types::CronUpdateRequest,
};
use crate::{Agent, tools::Tool};

pub(crate) struct CronUpdateTool {
    store: Arc<CronFileStore>,
    time_wheel: Arc<TimeWheel>,
    agent: Agent,
}

impl std::fmt::Debug for CronUpdateTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronUpdateTool")
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl Clone for CronUpdateTool {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            time_wheel: Arc::clone(&self.time_wheel),
            agent: self.agent.clone(),
        }
    }
}

impl CronUpdateTool {
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
impl Tool for CronUpdateTool {
    fn name(&self) -> &str {
        "cron_update"
    }

    fn description(&self) -> &str {
        "Update cron job."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "Job ID",
                },
                "crontab": {
                    "type": "string",
                    "description": "Crontab expression",
                },
                "description": {
                    "type": "string",
                    "description": "Description",
                },
                "content": {
                    "type": "string",
                    "description": "New content for the job",
                },
                "enabled": {
                    "type": "boolean",
                    "description": "Whether the job should be enabled or disabled",
                },
            },
            "required": vec!["job_id"],
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: CronUpdateRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let mut trigger = None;
        if let Some(ref crontab) = request.crontab {
            trigger = match parse_standard(crontab) {
                Ok(t) => Some(t),
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!("Invalid crontab: {e}"),
                    )]);
                }
            };
        }

        let old_job = match self.store.get_job(&request.job_id).await {
            Ok(j) => j,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to get job: {e}"
                ))]);
            }
        };

        let patch = CronJobPatch {
            crontab: request.crontab.clone(),
            description: request.description.clone(),
            content: request.content.clone(),
            enabled: request.enabled,
            next_run: None,
        };

        let job = match self.store.update_job(&request.job_id, &patch).await {
            Ok(j) => j,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to update job: {e}"
                ))]);
            }
        };

        let needs_reschedule =
            request.crontab.is_some() || request.enabled.is_some();

        if needs_reschedule {
            if old_job.enabled {
                self.time_wheel.remove(&old_job.id).await.ok();
            }

            if let Some(trigger_tmp) = trigger
                && job.enabled
            {
                let job_wrapper =
                    Arc::new(super::schedule::CronJobWrapper::new(
                        job.clone(),
                        Arc::clone(&self.store),
                        self.agent.clone(),
                    ));

                if let Err(e) = self
                    .time_wheel
                    .schedule_async(job_wrapper, Arc::from(trigger_tmp))
                    .await
                {
                    return CallToolResult::error(vec![Content::text(
                        format!("Failed to schedule job: {e}"),
                    )]);
                }
            }
        }

        let result = serde_json::to_value(&job).unwrap_or_default();
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        CallToolResult::success(vec![Content::text(json)])
    }
}
