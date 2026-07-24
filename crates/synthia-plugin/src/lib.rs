//! Synthia plugin system - core types and manifest parsing
//!
//! This crate provides:
//! - Plugin manifest definition and parsing
//! - Plugin registry for discovery and lifecycle management
//! - Plugin name and version validation
//! - Hook runner for event-based plugin hooks
//! - Error types for plugin operations

mod hook_runner;
mod manifest;
mod mcp_proxy;
mod registry;
mod types;

pub use hook_runner::{
    HookMetadata,
    HookRunner,
    HookRunnerConfig,
    SharedHookRunner,
};
pub use manifest::{PluginError, PluginManifest};
pub use mcp_proxy::{McpProxy, McpProxyError};
pub use registry::{
    HookConfig as PluginHookConfig,
    McpServerConfig,
    PluginHandle,
    PluginId,
    PluginPath,
    PluginRegistry,
};
pub use types::{
    FailMode,
    HookEvent,
    HookHandler,
    HookResult,
    HookSpec,
    McpConfigError,
    Transport,
};
