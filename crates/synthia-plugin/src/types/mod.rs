//! Types module for synthia-plugin

mod hook;
mod mcp;
pub use hook::{FailMode, HookEvent, HookHandler, HookResult, HookSpec};
pub use mcp::{McpConfigError, Transport};
