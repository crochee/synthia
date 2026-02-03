//! Task tools module
//!
//! Provides tools for managing persistent tasks stored in the database.

mod claim;
mod create;
mod data;
mod delegate;
mod delete;
mod file_store;
mod get;
mod list;
mod update;

pub mod shared;

use std::sync::Arc;

pub use claim::{
    ClaimTaskFailureReason,
    ClaimTaskRequest,
    ClaimTaskResult,
    ClaimTaskTool,
};
pub use create::TaskCreateTool;
pub use data::{
    Task,
    TaskMessage,
    TaskPacket,
    TaskPatch,
    TaskPriority,
    TaskStatus,
};
pub use delegate::TaskDelegateTool;
pub use delete::TaskDeleteTool;
pub use file_store::{TaskFileStore, TaskSummary};
pub use get::TaskGetTool;
pub use list::TaskListTool;
pub use update::TaskUpdateTool;

use crate::tools::{Tool, ToolRegistry};

pub async fn register_task_tools(registry: &ToolRegistry) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(TaskCreateTool::new()),
        Arc::new(TaskGetTool::new()),
        Arc::new(TaskListTool::new()),
        Arc::new(TaskUpdateTool::new()),
        Arc::new(TaskDeleteTool::new()),
        Arc::new(ClaimTaskTool::new()),
        Arc::new(TaskDelegateTool::new()),
    ];
    registry.registers(tools.into_iter()).await;
}
