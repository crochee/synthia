//! Subagent module
//!
//! Provides functionality to spawn isolated subagents for focused tasks.
//!
//! # Solo Mode Only
//!
//! The [`SubagentTool`] is designed exclusively for **Solo mode** agents.
//! It allows a standalone agent to spawn isolated subagents for focused,
//! short-lived tasks without the overhead of a full team structure.
//!
//! In **Team mode**, agents should use the team coordination tools instead:
//! - Team Leads use `spawn_teammate` to create persistent team members
//! - Team Members communicate via `send_message` and `read_inbox`
//!
//! # Subagent Permissions
//!
//! Subagents operate with restricted permissions for safety:
//!
//! ## Denied Tools
//!
//! The following tools are automatically denied for subagents:
//! - `subagent` - Prevents recursive subagent spawning
//! - `spawn_teammate` - Team management is Lead-only
//! - `broadcast` - Broadcasting is Lead-only
//! - `plan_approval` - Plan approval is Lead-only
//!
//! ## Allowed Tools
//!
//! Subagents have access to:
//! - File system tools (read, write, edit)
//! - Shell execution tools
//! - Web tools
//! - Memory tools
//! - Task tools (for reading, not claiming)
//!
//! # Usage Example
//!
//! ```ignore
//! // In Solo mode, spawn a subagent for a focused task
//! let request = SubagentRequest {
//!     task: "Fix the bug in auth.rs".to_string(),
//!     context: "The login function fails on empty passwords".to_string(),
//!     timeout: Some(300), // 5 minutes
//! };
//! let result = subagent_tool.call(request).await;
//! ```
//!
//! # Isolation Guarantees
//!
//! - Each subagent runs in its own context
//! - No shared state with the parent agent
//! - Results are returned to the parent upon completion
//! - Automatic cleanup after task completion or timeout

mod executor;
mod tool;
mod types;

pub use executor::{SubagentContext, SubagentExecutor};
pub use tool::SubagentTool;
pub use types::{ExecutorConfig, SubagentContextOverrides, SubagentRequest};

/// Agent alias for SubagentTool - allows both naming conventions to work
pub type Agent = SubagentTool;
