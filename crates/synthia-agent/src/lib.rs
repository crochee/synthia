#![cfg_attr(test, allow(deprecated))]
pub mod agent;
pub mod agent_file;
pub mod agent_instance;
pub mod agent_tools;
pub mod ask_user;
pub mod audit;
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
pub mod enhanced_dispatch;
pub mod error;
pub mod error_recovery;
pub mod event_log;
pub mod events;
pub mod hooks;
pub mod input;
pub mod loop_context;
#[cfg(feature = "unified-registry")]
pub mod loop_services;
pub mod memories;
pub mod memory_background_task;
pub mod observability;
pub mod panic_handler;
pub mod plugin_loader;
pub mod reasoning;
pub mod registry;
pub mod replay;
#[cfg(feature = "unified-registry")]
pub mod service_adapters;
pub mod shell;
pub mod steering;
pub mod stream_builder;
pub mod subagent;
pub mod task;
#[deprecated(
    note = "Use ToolOrchestrator instead; this module will be removed in a future release."
)]
pub mod tool_executor;
pub mod tool_registry;
pub mod tools;
pub mod tracing;
pub mod turn;
pub mod turn_transition;
pub mod types;
pub mod utils;

pub use agent::*;
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
#[deprecated(
    note = "Use ToolOrchestrator instead; these types will be removed in a future release."
)]
#[allow(deprecated)]
pub use enhanced_dispatch::{DispatcherConfig, EnhancedToolDispatcher};
pub use events::TokenUsage;
pub use hooks::HookExecutor;
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
    AgentInstance,
    AgentRegistry,
    AgentStatus,
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
pub use tool_registry::register_agent_tools;
pub use tools as agent_builtin_tools;
#[allow(deprecated)]
pub use tools::{ToolExecution, build_default_tool_registry, providers};
pub use tracing::{MetricsServer, ObservabilityConfig};
pub use types::*;
