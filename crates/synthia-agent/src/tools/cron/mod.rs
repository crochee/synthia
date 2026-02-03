//! Cron tools module
//!
//! This module provides tools for managing scheduled cron jobs.
//! - `cron_add`: Create a new scheduled job
//! - `cron_list`: List all scheduled jobs
//! - `cron_get`: Get details of a specific job
//! - `cron_remove`: Remove a scheduled job
//! - `cron_update`: Update an existing job
//! - `cron_run`: Force-run a job immediately
//! - `cron_runs`: List run history for a job

mod add;
mod data;
mod file_store;
mod get;
mod list;
mod remove;
mod run;
mod runs;
mod schedule;
mod types;
mod update;

use std::sync::Arc;

pub(crate) use add::CronAddTool;
pub use data::{CronJob, CronJobPatch, CronRun};
pub use file_store::CronFileStore;
pub(crate) use get::CronGetTool;
pub(crate) use list::CronListTool;
pub(crate) use remove::CronRemoveTool;
pub(crate) use run::CronRunTool;
pub(crate) use runs::CronRunsTool;
pub use schedule::CronJobWrapper;
use synthia_job::TimeWheel;
pub(crate) use update::CronUpdateTool;

use crate::{
    Agent,
    tools::{Tool, ToolRegistry},
};

/// Register all cron tools with the registry
pub async fn register_cron_tools(
    registry: &ToolRegistry,
    store: Arc<CronFileStore>,
    time_wheel: Arc<TimeWheel>,
    agent: Agent,
) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(CronAddTool::new(
            Arc::clone(&store),
            Arc::clone(&time_wheel),
            agent.clone(),
        )),
        Arc::new(CronListTool::new(Arc::clone(&store))),
        Arc::new(CronGetTool::new(Arc::clone(&store))),
        Arc::new(CronRemoveTool::new(
            Arc::clone(&store),
            Arc::clone(&time_wheel),
        )),
        Arc::new(CronUpdateTool::new(
            Arc::clone(&store),
            Arc::clone(&time_wheel),
            agent.clone(),
        )),
        Arc::new(CronRunTool::new(Arc::clone(&store), agent.clone())),
        Arc::new(CronRunsTool::new(Arc::clone(&store))),
    ];
    registry.registers(tools.into_iter()).await;
}
