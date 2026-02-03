//! Cron List Tool
//!
//! This tool allows listing all scheduled cron jobs.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use super::file_store::CronFileStore;
use crate::tools::Tool;

pub(crate) struct CronListTool {
    store: Arc<CronFileStore>,
}

impl std::fmt::Debug for CronListTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronListTool")
            .field("store", &"Arc<CronFileStore>")
            .finish()
    }
}

impl Clone for CronListTool {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
        }
    }
}

impl CronListTool {
    pub(crate) fn new(store: Arc<CronFileStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for CronListTool {
    fn name(&self) -> &str {
        "cron_list"
    }

    fn description(&self) -> &str {
        "List all cron jobs."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        let jobs = match self.store.list_jobs().await {
            Ok(j) => j,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to list jobs: {e}"
                ))]);
            }
        };
        let result = serde_json::to_value(&jobs).unwrap_or_default();
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        CallToolResult::success(vec![Content::text(json)])
    }
}
