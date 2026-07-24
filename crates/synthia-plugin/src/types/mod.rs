//! Types module for synthia-plugin

mod hook;
mod mcp;
#[allow(unused)]
pub use hook::{FailMode, HookEvent, HookHandler, HookResult, HookSpec};
pub use mcp::{McpConfigError, Transport};
