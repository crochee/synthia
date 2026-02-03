//! Background task tools module
//!
//! This module provides tools for managing background command execution.
//! Unlike cron jobs which are scheduled, background tasks run immediately
//! and their notifications are delivered to the agent.
//!
//! Features:
//! - `background_start`: Start a command in the background
//! - `background_status`: Check the status of a background task
//! - `background_list`: List all background tasks
//! - `background_stop`: Stop a running background task
//! - Notification queue for completed tasks

mod data;
mod file_store;
mod list;
mod start;
mod status;
mod stop;

use std::sync::Arc;

pub use data::BackgroundTask;
pub(crate) use list::BackgroundListTool;
pub(crate) use start::BackgroundStartTool;
pub(crate) use status::BackgroundStatusTool;
pub(crate) use stop::BackgroundStopTool;

use crate::{
    shell::ShellExecutor,
    tools::{Tool, ToolRegistry},
};

pub async fn register_background_tools(
    registry: &ToolRegistry,
    executor: Arc<dyn ShellExecutor>,
) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(BackgroundStartTool::new(Arc::clone(&executor))),
        Arc::new(BackgroundListTool::new()),
        Arc::new(BackgroundStatusTool::new()),
        Arc::new(BackgroundStopTool::new()),
    ];
    registry.registers(tools.into_iter()).await;
}
