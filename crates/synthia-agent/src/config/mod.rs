//! Configuration module
//!
//! This module provides unified configuration management for the agent.
//! It includes configurations for the agent itself, sessions, tools, and context management.

mod agent;
mod context;
mod prompt_cache;
mod remote_config;
mod session;
mod skill;
mod tool;

pub use agent::{AgentConfig, AgentName};
pub use context::{ContextConfig, ToolImportance, classify_tool_default};
pub use prompt_cache::{
    PromptCache1hCache,
    PromptCache1hConfig,
    add_session_to_prompt_cache_1h,
    clear_prompt_cache_1h,
    get_prompt_cache_1h_allowlist,
    is_session_eligible_for_prompt_cache_1h,
};
pub use remote_config::{
    ConfigSource,
    FeatureValue,
    Partial,
    RemoteConfigCache,
    clear_config_cache,
    get_dynamic_config_cached,
    get_feature_value_cached,
    is_config_cache_stale,
    refresh_remote_config_async,
    set_config_cache,
};
pub use session::SessionConfig;
pub use skill::SkillConfig;
pub use tool::ToolConfig;
