//! Context management module
//!
//! This module provides context management and compaction functionality to ensure
//! long-running sessions work smoothly.
//!
//! ## Architecture
//!
//! The context management system consists of:
//! - **ContextManager trait**: Core interface for context operations
//! - **DefaultContextManager**: Production implementation with 3-level compression
//! - **Token estimation**: Utilities for counting tokens
//! - **Pruning**: Message classification and removal strategies
//! - **Summarization**: LLM-based conversation summarization
//!
//! For detailed documentation, see [README.md](./README.md)

mod estimator;
mod manager;
mod normalize;
mod pruning;
pub mod realtime;
mod summarizer;
mod system_context;
mod transcript;
mod types;
mod user_context;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
pub use manager::DefaultContextManager;
use rmcp::model::SamplingMessage;
pub use types::{
    CompactionMetadata,
    CompactionResult,
    CompactionStrategy,
    SummaryQuality,
};

use crate::Result;

/// Trait for context management operations
#[async_trait]
pub trait ContextManager: Send + Sync {
    /// Compacts the conversation to reduce token usage
    ///
    /// Returns:
    /// - `Ok(None)` - No compaction needed
    /// - `Ok(Some(result))` - Compaction performed with detailed result
    async fn compact(
        &self,
        conversation: &[SamplingMessage],
    ) -> Result<Option<CompactionResult>>;

    /// Gets the most recent messages from the conversation
    async fn get_recent_messages(
        &self,
        max_messages: usize,
    ) -> Result<Vec<SamplingMessage>>;

    /// Performs mid-turn compaction to prune non-essential messages
    async fn mid_turn_compact(
        &self,
        messages: &mut Vec<SamplingMessage>,
        token_budget: usize,
    ) -> Result<bool>;
}

pub use estimator::estimate_tokens;
pub use normalize::{
    ensure_call_outputs_present,
    normalize_history,
    remove_orphan_outputs,
    strip_images_when_unsupported,
};
pub(crate) use pruning::micro_compact;
#[cfg(test)]
pub(crate) use pruning::{
    hard_clear_non_critical_tools,
    prune_tools_with_importance,
    soft_prune_all_tools,
};
pub use summarizer::{check_summary_quality, create_summary_message};
pub use system_context::{
    SystemContext,
    clear_system_context_cache,
    get_system_context,
};
pub use transcript::TranscriptManager;
pub use user_context::{
    UserContext,
    clear_user_context_cache,
    get_user_context,
};
