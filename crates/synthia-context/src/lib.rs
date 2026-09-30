//! # synthia-context
//!
//! Context-window management **and** the 3-layer memory tier
//! for the agent runtime.
//!
//! Two seams: the [`ContextManager`] trait (what fits in the
//! next prompt) and the [`Memory`] trait (what happened /
//! what we know). Implementations are split into strategies,
//! each ported from a reference design:
//!
//! | Module | Strategy | Ported from |
//! |---|---|---|
//! | [`context_manager`] | `NoopContextManager` (estimate-only) + `TruncatingContextManager` (pairwise drop) | traitclaw `ContextManager` |
//! | [`summarizing`] | `SummarizingContextManager` — summarise tool-result batches, replace with compact assistant summary, archive originals for `context_tree_query` | [`pi-context-prune`](https://github.com/championswimmer/pi-context-prune) |
//! | [`dag`] | `DagContextManager` — two-phase leaf + condensed hierarchical DAG compaction | [`pi-lcm`](https://github.com/codexstar69/pi-lcm) |
//! | [`memory`] | 3-layer `Memory` (conversation / working / long-term) + `SessionMemory` sink adapter + `FileMemory` scoped file tier + shared `events_to_messages` projection | traitclaw `Memory`, pi-subagents `memory.ts` |
//! ## Dependency direction
//!
//! ```text
//! synthia-harness ──▶ synthia-context ──▶ synthia-provider
//!                        │
//!                        ▼
//!                 synthia-session (SessionSink — conversation
//!                 tier of [`memory`])
//! ```
//!
//! `synthia-context` consumes the provider wire types
//! (`Message`, `ContentPart`, `ModelConfig`) and owns no runtime
//! of its own: every strategy is driven by the agent loop calling
//! [`ContextManager::prepare`] before each LLM sampling pass.
//!
//! ## Selection
//!
//! Strategies are plain `Arc<dyn ContextManager>` values; the
//! agent layer picks one at construction time
//! (`ReActAgent::with_context_manager`). The default agent ships
//! `TruncatingContextManager`; long-running coding sessions should
//! prefer `SummarizingContextManager` (cheap, tool-result
//! targeted) or `DagContextManager` (whole-conversation,
//! lossless via `expand`).

pub mod compaction_settings;
pub mod context_manager;
pub mod dag;
pub mod memory;
pub mod summarizing;

pub use compaction_settings::{
    CompactionDetails,
    CompactionSettings,
    ValidationError,
    should_compact,
};
pub use context_manager::{
    AgentState,
    ContextManager,
    NoopContextManager,
    SharedContextManager,
    TruncatingContextManager,
    UsageMeter,
};
pub use dag::{DagConfig, DagContextManager, DagNode, DagStore, SummaryId};
pub use memory::{
    FileMemory,
    FileMemoryConfig,
    FileMemoryEntry,
    FileMemoryError,
    InMemoryMemory,
    MAX_MEMORY_INDEX_LINES,
    Memory,
    MemoryEntry,
    MemoryError,
    MemoryKind,
    MemoryScope,
    SessionMemory,
    TypedProjectionError,
    events_to_messages,
    repair_session,
    typed_messages_from_sink,
};
pub use summarizing::{
    CompactionLifecycle,
    CompactionRecord,
    SUMMARY_TAG,
    SummariseFn,
    SummarizingContextManager,
    ToolCallArchive,
    ToolCallRecord,
};
