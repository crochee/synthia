//! Aggregated agent-tools surface.
//!
//! Historically this module was a single 1400+ line file holding
//! message-bus, coordinator, team, and tool implementations. It has
//! been split into focused sub-modules:
//!
//! - [`bus`]:            message bus + `AgentMessage`
//! - [`coordinator`]:    `AgentInstance` + `AgentCoordinator`
//! - [`team`]:           `Team` + `SubagentManager`
//! - [`agent_tool`]:     `Agent` tool
//! - [`messaging_tools`]: `SendMessage`, `TeamCreate`, `TeamDelete`
//! - [`lifecycle_tools`]: `Handoff`, `AgentStatus`, `RegisterAgent`
//!
//! All public types are re-exported from this module so existing
//! `use crate::tools::agent_tools::*` and `use crate::agent_tools::*`
//! import paths keep working without modification.

pub mod agent_tool;
pub mod builtin_types;
pub mod bus;
pub mod coordinator;
pub mod lifecycle_tools;
pub mod messaging_tools;
pub mod team;

#[cfg(test)]
mod tests;

pub use agent_tool::AgentTool;
pub use bus::{
    AgentMessage,
    InMemoryMessageBus,
    MessageBus,
    ReceiveError,
    SendError,
};
pub use coordinator::{AgentCoordinator, AgentError, AgentInstance};
pub use lifecycle_tools::{AgentStatusTool, HandoffTool, RegisterAgentTool};
pub use messaging_tools::{SendMessageTool, TeamCreateTool, TeamDeleteTool};
pub use team::{SlotGuard, SubagentManager, Team};
