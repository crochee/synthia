//! Agent module
//!
//! This module contains the core agent implementation, including:
//! - Main [`Agent`] struct and its methods
//! - ReAct loop logic for reasoning and acting
//! - Step processing for handling model calls and tool execution
//! - Conversation compaction for managing context window
//! - Model calling with retry logic
//! - Agent control plane for lifecycle management
//!
//! # Dual-Mode Architecture
//!
//! The agent supports two operating modes:
//!
//! ## Solo Mode (Default)
//!
//! A single agent operating independently with full access to all tools.
//! This is the traditional mode where the agent handles all tasks by itself.
//!
//! - Can use [`SubagentTool`] to spawn isolated subagents for focused tasks
//! - Has unrestricted access to all available tools
//! - Suitable for single-agent workflows and simple tasks
//!
//! ## Team Mode
//!
//! Multiple agents working together in a coordinated team structure.
//! Agents are organized with specific roles and restricted tool access.
//!
//! ### Team Roles
//!
//! - **Lead**: The team coordinator with elevated privileges
//!   - Can spawn teammates using `spawn_teammate`
//!   - Can broadcast messages to all teammates
//!   - Can approve/reject teammate plans
//!   - Has access to team management tools
//!
//! - **Member**: A worker agent with restricted privileges
//!   - Can send messages to the lead
//!   - Can read own inbox
//!   - Cannot spawn teammates or broadcast
//!   - Focused on executing assigned tasks
//!
//! # Mode Types
//!
//! - [`AgentMode`]: Configuration enum defining Solo or Team mode with role
//! - [`AgentModeState`]: Runtime state tracking current mode and task assignments
//! - [`TeamRole`]: Role enum for Team mode (Lead or Member)
//!
//! For detailed documentation, see [README.md](./README.md)
//!
//! [`SubagentTool`]: crate::tools::subagent::SubagentTool

pub mod builtins;
mod compact;
pub mod control;
mod core;
mod guards;
pub mod loop_detector;
mod model_call;
pub mod react;
pub mod reply;
mod step;
pub mod step_plan;
mod tool_executor;

pub use core::{Agent, AgentDeps};

pub use control::AgentControl;
pub use guards::{Guard, Guards};
pub use loop_detector::{
    LoopDetection,
    LoopDetector,
    LoopType,
    OperationPattern,
    Outcome,
};
pub use step_plan::{
    ExecutionMode,
    ExecutionPhase,
    ScheduleBuilder,
    ToolCallInfo,
    ToolSchedule,
};
