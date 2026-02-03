//! Cron Runs Tool
//!
//! This tool allows listing run history for a cron job.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use super::{file_store::CronFileStore, types::CronRunsRequest};
use crate::tools::Tool;

pub(crate) struct CronRunsTool {
    store: Arc<CronFileStore>,
}

impl std::fmt::Debug for CronRunsTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronRunsTool")
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl Clone for CronRunsTool {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
        }
    }
}

impl CronRunsTool {
    pub(crate) fn new(store: Arc<CronFileStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for CronRunsTool {
    fn name(&self) -> &str {
        "cron_runs"
    }

    fn description(&self) -> &str {
        "List cron job run history."
    }

    fn parameters(&self) -> Value {
        let schema = schemars::schema_for!(CronRunsRequest);
        serde_json::to_value(schema).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: CronRunsRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let limit = request.limit.unwrap_or(20);
        let runs = match self.store.list_runs(&request.job_id, limit).await {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to list runs: {e}"
                ))]);
            }
        };
        let result = serde_json::to_value(&runs).unwrap_or_default();
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        CallToolResult::success(vec![Content::text(json)])
    }
}
