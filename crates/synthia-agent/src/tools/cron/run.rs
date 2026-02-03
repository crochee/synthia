//! Cron Run Tool
//!
//! This tool allows force-running a cron job immediately.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use super::{file_store::CronFileStore, types::CronRunRequest};
use crate::{Agent, tools::Tool};

pub(crate) struct CronRunTool {
    store: Arc<CronFileStore>,
    agent: Agent,
}

impl std::fmt::Debug for CronRunTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronRunTool")
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl Clone for CronRunTool {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            agent: self.agent.clone(),
        }
    }
}

impl CronRunTool {
    pub(crate) fn new(store: Arc<CronFileStore>, agent: Agent) -> Self {
        Self { store, agent }
    }
}

#[async_trait]
impl Tool for CronRunTool {
    fn name(&self) -> &str {
        "cron_run"
    }

    fn description(&self) -> &str {
        "Run cron job immediately."
    }

    fn parameters(&self) -> Value {
        let schema = schemars::schema_for!(CronRunRequest);
        serde_json::to_value(schema).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: CronRunRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let job = match self.store.get_job(&request.job_id).await {
            Ok(j) => j,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to get job: {e}"
                ))]);
            }
        };

        let started_at = Utc::now();

        let (success, output) =
            super::schedule::execute_cron_job(&job, self.agent.clone()).await;
        let finished_at = Utc::now();
        let duration_ms = (finished_at - started_at).num_milliseconds();
        let status = if success { "ok" } else { "error" };

        if let Err(e) = self
            .store
            .record_run(
                &request.job_id,
                started_at,
                finished_at,
                status,
                Some(&output),
                duration_ms,
            )
            .await
        {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to record run: {e}"
            ))]);
        }

        let next = super::schedule::next_run(&job.crontab, finished_at)
            .unwrap_or_default();
        if let Err(e) = self
            .store
            .update_job_after_run(
                &request.job_id,
                next,
                finished_at,
                status,
                Some(&output),
            )
            .await
        {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to update job: {e}"
            ))]);
        }

        let result = serde_json::json!({
            "job_id": request.job_id,
            "success": success,
            "output": output
        });
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        CallToolResult::success(vec![Content::text(json)])
    }
}
