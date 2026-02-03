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
//! For detailed documentation, see [README.md](./README.md)

pub mod builtins;
mod compact;
pub mod control;
mod core;
mod guards;
pub mod loop_detector;
mod model_call;
pub mod react;
pub mod reply;
pub mod roles;
mod step;
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
pub use roles::{AgentRole, AgentRoleConfig};
