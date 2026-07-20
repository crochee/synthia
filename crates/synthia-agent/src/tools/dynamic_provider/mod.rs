//! Dynamic tool provider extension system.

pub mod extension_context;
pub mod extension_manager;
pub mod extension_points;
pub mod tool_provider;

pub use extension_context::{
    ExtensionContext,
    ExtensionContextSnapshot,
    ExtensionRuntime,
    PendingRegistration,
    SessionId,
    StaleContextError,
};
pub use extension_manager::ExtensionManager;
pub use extension_points::{
    Action,
    AfterHandler,
    AfterToolCall,
    AgentEnd,
    AgentLoopEvent,
    AgentLoopExtensionRegistry,
    AgentLoopHandler,
    AgentStart,
    BeforeHandler,
    BeforeToolCall,
    BranchNavigate,
    CompactEnd,
    CompactStart,
    DefinitionHandler,
    ErrorEvent,
    ErrorSeverity,
    ErrorSource,
    IterationEnd,
    IterationStart,
    SessionEnd,
    SessionStart,
    SyncAfterHandler,
    SyncBeforeHandler,
    SyncDefinitionHandler,
    ToolDefinitionView,
    ToolExtensionRegistry,
    TurnEnd,
    TurnStart,
};
pub use tool_provider::{
    HookEvent,
    SchemaRef,
    ToolDefinition,
    ToolPreCheck,
    ToolProvider,
};

/// Alias for the base `Tool` trait from `synthia-tool`.
pub trait Tool: synthia_tool::Tool {}
impl<T: synthia_tool::Tool> Tool for T {}
