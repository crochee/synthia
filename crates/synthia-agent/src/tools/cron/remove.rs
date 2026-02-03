//! Cron Remove Tool
//!
//! This tool allows removing a scheduled cron job.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;
use synthia_job::TimeWheel;

use super::{file_store::CronFileStore, types::CronRemoveRequest};
use crate::tools::Tool;

pub(crate) struct CronRemoveTool {
    store: Arc<CronFileStore>,
    time_wheel: Arc<TimeWheel>,
}

impl std::fmt::Debug for CronRemoveTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronRemoveTool")
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl Clone for CronRemoveTool {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            time_wheel: Arc::clone(&self.time_wheel),
        }
    }
}

impl CronRemoveTool {
    pub(crate) fn new(
        store: Arc<CronFileStore>,
        time_wheel: Arc<TimeWheel>,
    ) -> Self {
        Self { store, time_wheel }
    }
}

#[async_trait]
impl Tool for CronRemoveTool {
    fn name(&self) -> &str {
        "cron_remove"
    }

    fn description(&self) -> &str {
        "Remove cron job and its history."
    }

    fn parameters(&self) -> Value {
        let schema = schemars::schema_for!(CronRemoveRequest);
        serde_json::to_value(schema).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: CronRemoveRequest = match serde_json::from_value(args) {
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

        if job.enabled {
            self.time_wheel.remove(&request.job_id).await.ok();
        }

        if let Err(e) = self.store.remove_job(&request.job_id).await {
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to remove job: {e}"
            ))]);
        }

        let result = serde_json::json!({ "removed": request.job_id });
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        CallToolResult::success(vec![Content::text(json)])
    }
}
