// Legacy Tool trait usage during deprecation window (v3 toolification).
#![allow(deprecated)]

pub mod a2t;
pub mod agent;
pub mod agent_file;
pub mod agent_session;
pub mod ask_user;
pub mod audit;
pub(crate) mod builder;
pub mod checkpoint;
pub mod compaction;
pub mod component;
pub mod component_assembly;
pub mod config;
pub mod config_watcher;
pub mod context;
pub mod control;
pub mod dependencies;
pub mod doom_loop_handler;
pub mod error;
pub mod error_recovery;
pub mod event_log;
pub mod events;
pub mod executor;
pub mod handle;
pub mod hooks;
pub mod input;
pub mod interceptor;
pub mod loop_context;
pub mod loop_services;
pub mod memories;
pub mod memory_background_task;
pub mod observability;
pub mod panic_handler;
pub mod patterns;
pub mod plugin_loader;
pub mod reasoning;
pub mod registry;
pub mod replay;
pub(crate) mod resume;
pub mod service_adapters;
pub mod shell;
pub mod steering;
pub mod stream_builder;
pub mod subagent;
pub mod task;
pub mod tools;
pub mod tracing;
pub mod turn;
pub mod turn_transition;
pub mod types;
pub mod utils;

pub use a2t::{AgentAsTool, agent_as_tool};
pub use agent::*;
pub use agent_session::{AgentSession, CompactionState, LoopState};
pub use audit::*;
pub use component::{McpAssembler, ToolAssembler};
pub use component_assembly::ComponentAssembler;
pub use config::{
    AgentConfig,
    AgentConfigBuilder,
    AgentRunConfig,
    AgentRunConfigBuilder,
    AgentRunStateConfig,
    ObservabilityConfigInner,
};
pub use config_watcher::{
    ConfigChangeCallback,
    ConfigWatcher,
    HotReloadableFields,
    MultiConfigWatcher,
    SharedConfig,
    SynthiaConfig,
    resolve_all_config_paths,
    resolve_config_path,
    resolve_mcp_config_path,
    resolve_permission_config_path,
    resolve_provider_config_path,
    resolve_skill_config_path,
};
pub use context::{self as agent_context, VecMessageReader};
pub use events::TokenUsage;
pub use executor::{AgentExecutor, AgentStreamExecutor, RunConfig};
pub use handle::{AgentHandle, AgentHandleBuilder};
pub use hooks::HookExecutor;
pub use interceptor::{
    ApprovalInterceptor,
    CompactInterceptor,
    Interceptor,
    InterceptorChain,
    InterceptorContext,
    InterceptorError,
    InterceptorEvent,
    LoopDetectInterceptor,
    RetryInterceptor,
    TraceInterceptor,
};
pub use loop_context::LoopContext;
pub use memory_background_task::{
    MemoryBackgroundTask,
    graceful_shutdown,
    spawn,
};
pub use plugin_loader::{AgentPluginLoader, PluginLoaderError};
pub use reasoning::*;
pub use registry::{
    AgentDefinition,
    AgentFilter,
    AgentRegistry,
    AgentResult,
    AgentStatus,
    AgentTokenUsage,
    AgentToolWrapper,
};
pub use steering::{MpscSteeringChannel, SteeringChannel, SteeringMessage};
pub use subagent::{
    ChildSessionHandle,
    SubagentSessionError,
    SubagentSessionFactory,
    truncate_summary,
};
pub use synthia_memory::types::MemoryEvent;
pub use tools as agent_builtin_tools;
pub use tools::{ToolExecution, build_default_tool_registry, providers};
pub use tracing::{MetricsServer, ObservabilityConfig};
pub use types::*;
