//! Team tools module
//!
//! This module provides tools for managing agent teams in Team mode.
//!
//! # Team Mode Overview
//!
//! Team mode enables multiple agents to collaborate on complex tasks.
//! Unlike Solo mode (where a single agent uses [`SubagentTool`]), Team mode
//! creates persistent team members with specific roles and permissions.
//!
//! # Tool Categories
//!
//! ## Teammate Management (Lead Only)
//!
//! - `spawn_teammate`: Create a new teammate with specified role
//! - `list_teammates`: List all teammates in the team
//!
//! ## Communication Tools
//!
//! - `send_message`: Send message to a teammate or lead
//! - `read_inbox`: Read and drain inbox
//! - `broadcast`: Send message to all teammates (Lead only)
//!
//! ## Protocol Tools
//!
//! - `shutdown_request`: Request teammate to shut down
//! - `shutdown_response`: Check shutdown request status
//! - `plan_approval`: Approve/reject teammate's plan (Lead only)
//! - `idle`: Signal no more work
//!
//! ## Team Management Tools
//!
//! - `team_create`: Create a new team
//! - `team_list`: List all teams
//! - `team_assign`: Assign a task to a team
//! - `team_status`: Get team status
//! - `team_update`: Update team status or lead
//! - `team_delete`: Delete a team
//!
//! # Role-Based Access Control
//!
//! ## Solo Mode
//!
//! All team tools are denied. Use [`SubagentTool`] instead.
//!
//! ## Team Lead
//!
//! Full access to team coordination:
//! - Can spawn new teammates
//! - Can broadcast to all members
//! - Can approve/reject plans
//! - Can manage team lifecycle
//! - Can send direct messages
//!
//! ## Team Member
//!
//! Restricted access for task execution:
//! - Can send messages to lead
//! - Can read own inbox
//! - Cannot spawn teammates
//! - Cannot broadcast
//! - Cannot approve plans
//!
//! # Usage Example
//!
//! ```ignore
//! // Lead spawns a new member
//! spawn_teammate({
//!     "role": "member",
//!     "task": "Implement authentication"
//! })
//!
//! // Member sends progress to lead
//! send_message({
//!     "to": "lead",
//!     "content": "Authentication module 50% complete"
//! })
//!
//! // Lead broadcasts to all
//! broadcast({
//!     "content": "Team meeting in 5 minutes"
//! })
//! ```
//!
//! [`SubagentTool`]: crate::tools::subagent::SubagentTool

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
pub use types::MemberConfig;

use crate::{
    config::AgentName,
    tools::{Tool, ToolRegistry},
};

/// Register team tools with default (Solo) name.
pub async fn register_team_tools(registry: &ToolRegistry) {
    register_team_tools_with_name(registry, AgentName::default()).await;
}

/// Register team tools with a specific agent name.
pub async fn register_team_tools_with_name(
    registry: &ToolRegistry,
    name: AgentName,
) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(
            teammate::SpawnTeammateTool::new().with_parent_name(name.clone()),
        ),
        Arc::new(teammate::ListTeammatesTool::new()),
        Arc::new(
            message::SendMessageTool::new()
                .with_parent_name(name.clone())
                .with_agent_name("lead".to_string()),
        ),
        Arc::new(
            message::ReceiveMessageTool::new()
                .with_agent_name("lead".to_string()),
        ),
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

// =============================================================================
// Tool Factory Functions for Mode-Aware Registration
// =============================================================================

/// Create a SpawnTeammateTool instance
pub fn create_spawn_teammate_tool() -> Arc<dyn Tool> {
    Arc::new(teammate::SpawnTeammateTool::new())
}

/// Create a ListTeammatesTool instance
pub fn create_list_teammates_tool() -> Arc<dyn Tool> {
    Arc::new(teammate::ListTeammatesTool::new())
}

/// Create a SendMessageTool instance
pub fn create_send_message_tool(agent_name: Option<String>) -> Arc<dyn Tool> {
    match agent_name {
        Some(name) => {
            Arc::new(message::SendMessageTool::new().with_agent_name(name))
        }
        None => Arc::new(message::SendMessageTool::new()),
    }
}

/// Create a ReceiveMessageTool instance
pub fn create_read_inbox_tool(agent_name: Option<String>) -> Arc<dyn Tool> {
    match agent_name {
        Some(name) => {
            Arc::new(message::ReceiveMessageTool::new().with_agent_name(name))
        }
        None => Arc::new(message::ReceiveMessageTool::new()),
    }
}

/// Create a BroadcastTool instance
pub fn create_broadcast_tool() -> Arc<dyn Tool> {
    Arc::new(message::BroadcastTool::new())
}

/// Create a ShutdownRequestTool instance
pub fn create_shutdown_request_tool() -> Arc<dyn Tool> {
    Arc::new(protocol::ShutdownRequestTool::new())
}

/// Create a ShutdownResponseTool instance
pub fn create_shutdown_response_tool() -> Arc<dyn Tool> {
    Arc::new(protocol::ShutdownResponseTool::new())
}

/// Create a PlanApprovalTool instance
pub fn create_plan_approval_tool() -> Arc<dyn Tool> {
    Arc::new(protocol::PlanApprovalTool::new())
}

/// Create an IdleTool instance
pub fn create_idle_tool() -> Arc<dyn Tool> {
    Arc::new(idle::IdleTool)
}

/// Create a TeamCreateTool instance
pub fn create_team_create_tool() -> Arc<dyn Tool> {
    Arc::new(team_management::TeamCreateTool::new())
}

/// Create a TeamListTool instance
pub fn create_team_list_tool() -> Arc<dyn Tool> {
    Arc::new(team_management::TeamListTool::new())
}

/// Create a TeamAssignTool instance
pub fn create_team_assign_tool() -> Arc<dyn Tool> {
    Arc::new(team_management::TeamAssignTool::new())
}

/// Create a TeamStatusTool instance
pub fn create_team_status_tool() -> Arc<dyn Tool> {
    Arc::new(team_management::TeamStatusTool::new())
}

/// Create a TeamUpdateTool instance
pub fn create_team_update_tool() -> Arc<dyn Tool> {
    Arc::new(team_management::TeamUpdateTool::new())
}

/// Create a TeamDeleteTool instance
pub fn create_team_delete_tool() -> Arc<dyn Tool> {
    Arc::new(team_management::TeamDeleteTool::new())
}
