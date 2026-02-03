//! Subagent module
//!
//! Provides functionality to spawn isolated subagents for focused tasks.

mod executor;
mod tool;
mod types;

pub use executor::{SubagentContext, SubagentExecutor};
pub use tool::SubagentTool;
pub use types::{ExecutorConfig, SubagentContextOverrides, SubagentRequest};
