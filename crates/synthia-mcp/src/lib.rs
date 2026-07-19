// Legacy Tool trait usage during deprecation window (v3 toolification).
#![allow(deprecated)]

pub(crate) mod client;
pub mod config;
pub mod connection;
pub mod discovery;
pub(crate) mod jsonrpc;
pub mod manager;
pub mod mcp_tool;
pub mod oauth;
pub mod registry;
pub mod server;
pub mod tool_adapter;
pub mod types;

pub use client::{
    call_tool,
    initialize_server,
    list_tools,
    send_request_with_auth_retry,
};
pub use config::*;
pub use connection::*;
pub use discovery::*;
pub use manager::*;
pub use mcp_tool::*;
pub use oauth::*;
pub use registry::*;
pub use server::*;
pub use tool_adapter::*;
pub use types::*;
