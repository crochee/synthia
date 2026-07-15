//! Tools module - agent tool implementations
pub mod agent_tools;
pub mod builtins;
pub mod compact_context;
pub mod dynamic_provider;
pub mod orchestrator;
pub mod providers;
pub mod registry;
pub mod self_reflect;
pub mod static_tool_adapter;
pub mod tool_execution;
pub mod tool_runtime;

// Re-export main types
pub use agent_tools::AgentTool;
pub use compact_context::CompactContextTool;
#[allow(deprecated)]
pub use registry::build_default_tool_registry;
pub use self_reflect::SelfReflectTool;
pub use tool_execution::{
    ToolExecution,
    extract_output_text,
    normalize_tool_outputs,
};
