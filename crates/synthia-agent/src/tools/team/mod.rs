//! Team tools module
//!
//! This module provides tools for managing agent teams.
//! - `spawn_teammate`: Create a new teammate
//! - `list_teammates`: List all teammates
//! - `send_message`: Send message to a teammate
//! - `read_inbox`: Read and drain inbox
//! - `broadcast`: Send message to all teammates
//! - `shutdown_request`: Request teammate to shut down
//! - `shutdown_response`: Check shutdown request status
//! - `plan_approval`: Approve/reject teammate's plan
//! - `idle`: Signal no more work
//!
//! Team Management Tools:
//! - `team_create`: Create a new team
//! - `team_list`: List all teams
//! - `team_assign`: Assign a task to a team
//! - `team_status`: Get team status
//! - `team_update`: Update team status or lead
//! - `team_delete`: Delete a team

mod data;
pub(crate) mod file_store;
mod idle;
mod message;
pub(crate) mod message_store;
mod protocol;
pub(crate) mod shared;
mod team_management;
pub(crate) mod teammate;
pub(crate) mod tool_base;
pub mod types;

#[cfg(test)]
mod tests;

use std::sync::Arc;

pub use data::{
    AgentStatus,
    MessageType,
    PlanRequest,
    ShutdownRequest,
    Team,
    TeamMessage,
    TeamPatch,
    TeamStatus,
    Teammate,
    TeammateStatus,
};
pub use file_store::TeamStorage;

use crate::tools::{Tool, ToolRegistry};

pub async fn register_team_tools(registry: &ToolRegistry) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(teammate::SpawnTeammateTool::new()),
        Arc::new(teammate::ListTeammatesTool::new()),
        Arc::new(message::SendMessageTool::new()),
        Arc::new(message::ReadInboxTool::new()),
        Arc::new(message::BroadcastTool::new()),
        Arc::new(protocol::ShutdownRequestTool::new()),
        Arc::new(protocol::ShutdownResponseTool::new()),
        Arc::new(protocol::PlanApprovalTool::new()),
        Arc::new(idle::IdleTool),
        Arc::new(team_management::TeamCreateTool::new()),
        Arc::new(team_management::TeamListTool::new()),
        Arc::new(team_management::TeamAssignTool::new()),
        Arc::new(team_management::TeamStatusTool::new()),
        Arc::new(team_management::TeamUpdateTool::new()),
        Arc::new(team_management::TeamDeleteTool::new()),
    ];
    registry.registers(tools.into_iter()).await;
}
