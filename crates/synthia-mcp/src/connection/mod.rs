//! MCP connection management — connection state tracking,
//! lifecycle management, and tool interaction.
//!
//! # Module Layout
//!
//! - [`state`]: [`state::ConnectionState`] enum + `Display`
//!   impl.
//! - [`connection`]: [`connection::McpConnection`] struct +
//!   `impl McpConnection` (14 methods: `new`, `connect`,
//!   `establish_connection`, `call_tool`, `disconnect`,
//!   `disconnect_graceful`, `update_last_used`,
//!   `last_used_duration`, `refresh_tools`, `is_dead`,
//!   `mark_dead`, `get_tools`, `tools_mut`, `state`,
//!   `is_connected`) + `Debug` impl.
//! - [`tests`]: 7 unit tests covering `ConnectionState`
//!   display/equality, `McpConnection` new/disconnect/
//!   last_used/tools/tools_mut/debug.

#[allow(clippy::module_inception)]
mod connection;
mod state;

#[cfg(test)]
mod tests;

pub use connection::McpConnection;
pub use state::ConnectionState;
