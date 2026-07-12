//! Agent-loop extension points — typed hook points fired by the agent
//! runtime at well-defined lifecycle events.
//!
//! See [`agent_loop`] for the Agent Loop + Tool scopes (Phase 3, 21
//! points), [`llm`] for the LLM scope (Round 1, 8 points), [`context`]
//! for the Context scope (Round 1, 7 points), [`permission`] for the
//! Permission scope (Round 2, 5 points), [`provider`] for the
//! Provider scope (Round 2, 4 points), [`event_bus`] for the Event
//! Bus scope (Round 3, 4 points), [`plugin_lifecycle`] for the
//! Plugin Lifecycle scope (Round 3, 6 points), [`session_tree`] for
//! the Session Tree scope (Round 4, 5 points), and [`output_ui`] for
//! the Output/UI scope (Round 4, 4 points).

pub mod agent_loop;
pub mod context;
pub mod event_bus;
pub mod llm;
pub mod output_ui;
pub mod permission;
pub mod plugin_lifecycle;
pub mod provider;
pub mod session_tree;
pub mod tool;

pub use agent_loop::{
    AgentEnd,
    AgentLoopEvent,
    AgentLoopExtensionRegistry,
    AgentLoopHandler,
    AgentStart,
    BranchNavigate,
    CompactEnd,
    CompactStart,
    ErrorEvent,
    ErrorSeverity,
    ErrorSource,
    IterationEnd,
    IterationStart,
    SessionEnd,
    SessionStart,
    TurnEnd,
    TurnStart,
};
pub use context::{
    CompactPlan,
    CompactReplaceHandler,
    CompactStrategy,
    CompactTriggerHandler,
    CompactTriggerInput,
    ContextExtensionRegistry,
    ContextObservabilityEvent,
    MessageFilterHandler,
    MessageFilterInput,
    ObservabilityEmitHandler,
    PrefixParticipateHandler,
    SummarizeHandler,
    SummarizeInput,
    TokenBudget,
    TokenBudgetHandler,
};
pub use event_bus::{
    AggregateHandler,
    AggregateRequest,
    AggregatedEvent,
    EventBusExtensionRegistry,
    EventHandler,
    EventTopic,
    PublishRequest,
    ReplayHandler,
    ReplayRequest,
    ReplayedEvent,
    SubscribeHandler,
    SubscribeRequest,
};
pub use llm::{
    CacheBreakpoint,
    CacheBreakpointHandler,
    CacheBreakpointInput,
    ChatHeadersHandler,
    ChatHeadersInput,
    ChatParams,
    ChatParamsHandler,
    LlmExtensionRegistry,
    MessagesHandler,
    MessagesTransformInput,
    ModelSelectHandler,
    ModelSelectInput,
    ResponseTransformHandler,
    ResponseTransformInput,
    SystemPromptHandler,
    SystemPromptTransformInput,
    ToolChoiceHandler,
    ToolChoiceInput,
};
pub use output_ui::{
    Audience,
    ComponentKind,
    ConfirmRequest,
    DialogConfirmHandler,
    DialogNotifyHandler,
    HostKind,
    MetadataPatch,
    MetadataValue,
    MimeType,
    NotificationLevel,
    NotifyRequest,
    OutputFormatHandler,
    OutputFormatInput,
    OutputUiExtensionRegistry,
    RenderComponentHandler,
    RenderOutput,
    RenderRequest,
};
pub use permission::{
    BlacklistEntry,
    BlacklistHandler,
    BlacklistInput,
    DoomLoopAction,
    DoomLoopHandler,
    DoomLoopInfo,
    PermissionAskHandler,
    PermissionDecision,
    PermissionExtensibilityGuard,
    PermissionExtensionRegistry,
    PermissionNotifyHandler,
    PermissionNotifyInput,
    PermissionRequest,
    PersistHandler,
    PersistInput,
    PersistOutput,
};
pub use plugin_lifecycle::{
    BindHandler,
    BindRequest,
    DualForm,
    DualFormHandler,
    DualFormQuery,
    DualFormResponse,
    HotSwapHandler,
    HotSwapRequest,
    InvalidateHandler,
    InvalidateRequest,
    LoadHandler,
    LoadRequest,
    PluginLifecycleExtensionRegistry,
    UnloadHandler,
    UnloadRequest,
};
pub use provider::{
    AuthHandler,
    AuthRequest,
    FallbackChain,
    FallbackContext,
    FallbackHandler,
    ProviderConfig,
    ProviderExtensionRegistry,
    RegisterHandler,
    UnregisterHandler,
};
pub use session_tree::{
    BranchCreateHandler,
    BranchCreateOutput,
    BranchCreateRequest,
    BranchFrozenError,
    BranchNode,
    CompactionEvent,
    CompactionPreserveHandler,
    EntryAppendHandler,
    EntryAppendInput,
    EntryId,
    MigrateRequest,
    SessionEntry,
    SessionId,
    SessionTreeExtensionRegistry,
    TreeWalkHandler,
    TreeWalkRequest,
    VersionMigrateHandler,
};
pub use tool::{
    Action,
    AfterHandler,
    AfterToolCall,
    BeforeHandler,
    BeforeToolCall,
    DefinitionHandler,
    ToolDefinitionView,
    ToolExtensionRegistry,
};
