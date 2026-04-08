//! Task tools module
//!
//! Provides tools for managing persistent tasks stored in the database.
//!
//! # Task Management
//!
//! Tasks are persistent work items that can be:
//! - Created with dependencies and priorities
//! - Assigned to agents or teams
//! - Tracked through their lifecycle
//! - Completed with output records
//!
//! # Available Tools
//!
//! - `task_create`: Create a new task with optional dependencies
//! - `task_get`: Retrieve a specific task by ID
//! - `task_list`: List tasks with optional filters
//! - `task_update`: Update task properties
//! - `task_delete`: Delete a task
//! - `task_stop`: Stop a running task
//! - `claim_task`: Atomically claim an available task
//! - `task_delegate`: Delegate a task to another agent
//!
//! # Task Preemption Mechanism
//!
//! The `claim_task` tool implements atomic task claiming with preemption support:
//!
//! ## Claim Process
//!
//! 1. **Dependency Check**: Uses [`TaskGraph`] to verify all dependencies are completed
//! 2. **Availability Check**: Ensures task is not already claimed by another agent
//! 3. **Busy Check**: Optionally verifies the claiming agent isn't already busy
//! 4. **Atomic Claim**: Uses locks to prevent race conditions
//!
//! ## Preemption Prevention
//!
//! ```ignore
//! // Atomic claim prevents race conditions
//! claim_task({
//!     "task_id": "task-1",
//!     "owner": "agent-1",
//!     "check_busy": true  // Fail if agent already has open tasks
//! })
//! ```
//!
//! ## Failure Reasons
//!
//! - `TaskNotFound`: Task does not exist
//! - `AlreadyClaimed`: Task is owned by another agent
//! - `AlreadyResolved`: Task is already completed
//! - `Blocked`: Dependencies not yet satisfied
//! - `AgentBusy`: Agent has other open tasks (when check_busy=true)
//! - `NoAvailableTasks`: No tasks match criteria (for claim_available)
//!
//! # TaskGraph Usage
//!
//! [`TaskGraph`] provides dependency management:
//!
//! ## Dependency Tracking
//!
//! - `blocked_by`: Tasks that must complete before this task
//! - `blocks`: Tasks that depend on this task
//!
//! ## Graph Operations
//!
//! - `topological_sort()`: Get execution order respecting dependencies
//! - `get_ready_tasks()`: Find tasks with satisfied dependencies
//! - `check_dependencies()`: Verify if a task can be claimed
//! - `find_cycles()`: Detect circular dependencies
//! - `get_critical_path()`: Find longest dependency chain
//!
//! ## Priority-Based Execution
//!
//! ```ignore
//! // Tasks are claimed by priority (highest first)
//! // Critical > High > Normal > Low
//! claim_available_task({
//!     "team_id": "team-1",
//!     "owner": "agent-1"
//! })
//! ```
//!
//! # Task States
//!
//! - `Pending`: Task is waiting to be claimed
//! - `InProgress`: Task is being worked on
//! - `Completed`: Task finished successfully
//! - `Failed`: Task execution failed
//! - `Stopped`: Task was stopped before completion

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
    ClaimAvailableTaskRequest,
    ClaimTaskFailureReason,
    ClaimTaskRequest,
    ClaimTaskResult,
    ClaimTaskTool,
};
pub use create::TaskCreateTool;
pub use data::{
    Task,
    TaskGraph,
    TaskMessage,
    TaskNode,
    TaskPacket,
    TaskPatch,
    TaskPriority,
    TaskStatus,
    TopologicalSortResult,
};
pub use delegate::TaskDelegateTool;
pub use delete::{TaskDeleteTool, TaskStopTool};
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
        Arc::new(TaskStopTool::new()),
        Arc::new(ClaimTaskTool::new()),
        Arc::new(TaskDelegateTool::new()),
    ];
    registry.registers(tools.into_iter()).await;
}

// =============================================================================
// Tool Factory Functions for Mode-Aware Registration
// =============================================================================

/// Create a TaskCreateTool instance
pub fn create_task_create_tool() -> Arc<dyn Tool> {
    Arc::new(TaskCreateTool::new())
}

/// Create a TaskGetTool instance
pub fn create_task_get_tool() -> Arc<dyn Tool> {
    Arc::new(TaskGetTool::new())
}

/// Create a TaskListTool instance
pub fn create_task_list_tool() -> Arc<dyn Tool> {
    Arc::new(TaskListTool::new())
}

/// Create a TaskUpdateTool instance
pub fn create_task_update_tool() -> Arc<dyn Tool> {
    Arc::new(TaskUpdateTool::new())
}

/// Create a TaskDeleteTool instance
pub fn create_task_delete_tool() -> Arc<dyn Tool> {
    Arc::new(TaskDeleteTool::new())
}

/// Create a TaskStopTool instance
pub fn create_task_stop_tool() -> Arc<dyn Tool> {
    Arc::new(TaskStopTool::new())
}

/// Create a ClaimTaskTool instance
pub fn create_claim_task_tool() -> Arc<dyn Tool> {
    Arc::new(ClaimTaskTool::new())
}

/// Create a TaskDelegateTool instance
pub fn create_task_delegate_tool() -> Arc<dyn Tool> {
    Arc::new(TaskDelegateTool::new())
}
