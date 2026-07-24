//! Data definitions for the REPL.
//!
//! This module owns every struct and enum that the other
//! `repl::` submodules operate on. Field visibility is
//! `pub(super)` so the impl blocks scattered across
//! [`construct`], [`run_loop`], [`handle_command`],
//! [`format_event`], [`execute`], and [`agent_message`]
//! can manipulate state directly while the public
//! surface (re-exported from `repl::mod`) stays narrow.
//!
//! Trivial `Default` impls live here too so the
//! `Default` derive line stays next to the data.

use std::{path::PathBuf, sync::Arc};

use parking_lot::RwLock;
use synthia_agent::TokenUsage;
use synthia_command::CommandRegistry;
use synthia_memory::{episodic::EpisodicMemory, hot::HotMemory};
use synthia_provider::{config::WorkspaceConfig, traits::ModelProvider};

use crate::commands::CliCommand;

/// Session state tracking for prompt display and status (Task 10.16).
pub struct SessionState {
    pub(super) iteration_count: usize,
    pub(super) tool_call_count: usize,
    pub(super) token_usage: TokenUsage,
    pub(super) mode: crate::commands::AgentMode,
    pub(super) theme: crate::theme::Theme,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for REPL instance.
pub struct ReplConfig {
    pub workspace_root: PathBuf,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
        }
    }
}

/// An interactive REPL that reads user input, routes slash commands, and formats agent events for display.
pub struct Repl {
    pub command_registry: CommandRegistry,
    pub workspace_root: PathBuf,
    pub state: RwLock<SessionState>,
}

/// Action returned by command parsing to drive the REPL dispatch loop.
#[derive(Debug)]
pub enum CommandAction {
    Quit,
    Clear,
    Help,
    Mode(Option<String>),          // Task 10.8
    Status,                        // Task 10.9
    Compact,                       // Task 10.10
    MemoryDisplay(Option<String>), // Task 10.12
    Execute(CliCommand),
    AgentMessage(String),
    Empty,
}

/// Context needed by the REPL to maintain state between iterations.
pub struct ReplContext {
    pub provider: Option<Arc<dyn ModelProvider>>,
    pub workspace_root: PathBuf,
    pub workspace_config: WorkspaceConfig,
    pub current_model: String,
    pub current_provider_name: String,
    pub session_id: String,
    pub hot_memory: Option<Arc<HotMemory>>,
    pub episodic_memory: Option<Arc<EpisodicMemory>>,
    pub skill_summaries: Option<String>,
}
