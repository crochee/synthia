//! Cron Get Tool
//!
//! This tool allows getting details of a specific cron job.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use super::{file_store::CronFileStore, types::CronGetRequest};
use crate::tools::Tool;

pub(crate) struct CronGetTool {
    store: Arc<CronFileStore>,
}

impl std::fmt::Debug for CronGetTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronGetTool")
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl Clone for CronGetTool {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
        }
    }
}

impl CronGetTool {
    pub(crate) fn new(store: Arc<CronFileStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for CronGetTool {
    fn name(&self) -> &str {
        "cron_get"
    }

    fn description(&self) -> &str {
        "Get cron job details by ID."
    }

    fn parameters(&self) -> Value {
        let schema = schemars::schema_for!(CronGetRequest);
        serde_json::to_value(schema).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: CronGetRequest = match serde_json::from_value(args) {
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

        let result = serde_json::to_value(&job).unwrap_or_default();
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        CallToolResult::success(vec![Content::text(json)])
    }
}
