//! # Synthia Agent
//!
//! A powerful AI agent implementation with tool support, conversation management,
//! and extensible architecture.
//!
//! ## Documentation
//!
//! For detailed documentation, see the module-specific README files in each module directory.
//!
//! ## Overview
//!
//! This crate provides the core agent functionality for the Synthia AI system.
//! It includes:
//!
//! - **Agent**: The main agent implementation with React-style reasoning loop
//! - **Session Management**: In-memory session storage with LRU eviction
//! - **Tool System**: Extensible tool registry with MCP support
//! - **Context Management**: Token-aware context window management
//! - **Hooks**: Event-driven hook system for extensibility
//! - **Scheduler**: Background cron job scheduler for automated task execution
//! - **Guardian**: Safety and approval system for sensitive operations
//! - **Model Router**: Intelligent model selection based on task complexity
//!
//! ## Module Structure
//!
//! ```text
//! synthia-agent
//! ├── agent          # Core agent implementation with ReAct loop
//! ├── config         # Configuration management
//! ├── context        # Context window management and compression
//! ├── error          # Error types and handling
//! ├── event_handler  # Event processing system
//! ├── guardian       # Safety and approval system
//! ├── hooks          # Extensibility hooks
//! ├── memories       # Memory management system
//! ├── model_router   # Model selection and routing
//! ├── prompt         # System prompt building
//! ├── session        # Session management
//! ├── shell          # Shell execution abstraction
//! ├── storage        # Data persistence layer
//! ├── tools          # Tool implementations
//! ├── types          # Core type definitions
//! └── utils          # Utility functions
//! ```
//!
//! ## Example
//!
//! ```rust,ignore
//! use synthia_agent::{Agent, config::AgentConfig, agent::AgentDeps};
//! use synthia_agent::context::DefaultContextManager;
//! use synthia_agent::hooks::HookRegistry;
//! use synthia_agent::model_router::FirstModelRouter;
//! use synthia_agent::tools::{SkillTool, ToolRegistry};
//! use synthia_agent::guardian::{Guardian, SimpleGuardian, GuardianConfig};
//! use synthia_agent::agent::AgentControl;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create agent dependencies
//!     let deps = AgentDeps {
//!         tools: Arc::new(ToolRegistry::new()),
//!         context: Arc::new(DefaultContextManager::new(Arc::new(FirstModelRouter::default()))),
//!         session: Arc::new(synthia_agent::session::SessionFileStore::new()),
//!         router: Arc::new(FirstModelRouter::default()),
//!         hooks: Arc::new(HookRegistry::new()),
//!         skills: Arc::new(SkillTool::new(std::path::PathBuf::from("."))),
//!         guardian: Arc::new(SimpleGuardian::new(GuardianConfig::default())) as Arc<dyn Guardian>,
//!         control: Arc::new(AgentControl::new()),
//!     };
//!
//!     // Create agent with config and dependencies
//!     let agent = Agent::new(Arc::new(AgentConfig::default()), deps);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! - **ReAct Loop**: Reasoning and Acting pattern for agent decision making
//! - **Tool System**: Extensible tool registry with file system, web, and custom tools
//! - **Context Management**: Automatic context window compression with 4-level strategy
//! - **MCP Support**: Model Context Protocol integration for external tools
//! - **Memory System**: Short-term and long-term memory for conversation context
//! - **Cron Scheduler**: Background task scheduling for automated execution
//! - **Multi-agent**: Team collaboration and sub-agent spawning
//! - **Safety**: Guardian system for approval of sensitive operations
//!
//! ## Testing
//!
//! Run all tests:
//! ```bash
//! cargo test -p synthia-agent --lib
//! ```
//!
//! Run specific module tests:
//! ```bash
//! cargo test -p synthia-agent agent:: --lib
//! cargo test -p synthia-agent context:: --lib
//! cargo test -p synthia-agent tools:: --lib
//! ```

pub mod agent;
pub mod config;
pub mod context;
pub mod error;
pub mod event_handler;
pub mod guardian;
pub mod hooks;
pub mod memories;
pub mod model_router;
pub mod prompt;
pub mod session;
pub mod shell;
pub mod tools;
pub mod types;
pub mod utils;

// Re-export tool submodules for convenience
// =============================================================================
// Core Types
// =============================================================================
pub use agent::Agent;
pub use error::AgentError;
pub use event_handler::AgentEventHandler;
// =============================================================================
// Guardian Types
// =============================================================================
pub use guardian::GuardianConfig;
// =============================================================================
// Memory Types
// =============================================================================
pub use memories::{
    Memory,
    MemoryFileStore,
    MemoryImportance,
    MemoryQuery,
    MemoryStats,
    MemoryStore,
    MemoryType,
    Stage1Output,
};
// =============================================================================
// Session Types
// =============================================================================
pub use session::{Session, SessionFileStore, SessionManager};
// =============================================================================
// Tool Types
// =============================================================================
pub use tools::{
    AskUserQuestionTool,
    CronJobWrapper,
    ExecutorConfig,
    Question,
    QuestionAnswer,
    QuestionOption,
    QuestionRequest,
    QuestionResponse,
    QuestionSenderImpl,
    SkillTool,
    SubagentTool,
    ToolRegistry,
    get_mcp_tools,
    register_background_tools,
    register_cron_tools,
    register_task_tools,
    register_team_tools,
    register_worktree_tools,
};
pub use tools::{fs, thinking, todo, tom, web};

// =============================================================================
// Result Type
// =============================================================================

pub type Result<T, E = AgentError> = core::result::Result<T, E>;
