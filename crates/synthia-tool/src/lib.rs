pub mod builtin;
pub mod registry;
pub mod traits;
pub mod truncate;
pub mod types;

#[cfg(test)]
mod tool_test;
#[cfg(test)]
mod types_test;

pub use builtin::{
    DEFAULT_DENY_PATTERNS,
    ReadTool,
    ShellSafetyConfig,
    ShellTool,
    TodoWriteTool,
    WebFetchTool,
    WriteTool,
    build_default_tool_registry,
};
pub use registry::{
    RegistrationScope,
    RegistrationToken,
    ToolCategory,
    ToolDescriptor,
    ToolEntry,
    ToolExposure,
    ToolMetadataSnapshot,
    ToolProvenance,
    ToolRegistry,
};
pub use traits::{ExecutionMode, StreamOutput, Tool};
pub use truncate::{
    OutputBound,
    OverflowStrategy,
    SanitizationPolicy,
    bound_output,
    start_cleanup_task,
};
pub use types::{Context, DispatchMode, Result, ToolOutput, TruncatedBy};
