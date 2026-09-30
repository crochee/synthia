//! `synthia-session` — durable session storage for the agent runtime.
//!
//! ## Public surface
//!
//! This crate used to expose ~30 types (`SessionManager`,
//! `Session`, `SessionStateMachine`, `Store`, …) plus state
//! machine, token budget, and approval modules. After the
//! refactor the public surface collapses to:
//!
//! - [`SessionSink`] — the single trait every consumer
//!   (typically `synthia-harness`) depends on. **5 required
//!   methods** (`id`, `append`, `read`, `snapshot`, `close`)
//!   plus 2 **default-implemented** methods
//!   (`append_with_key`, `append_many`) that opt into
//!   idempotency and batched durability without forcing every
//!   backend to implement them.
//! - [`SessionError`] — the only error type a sink returns.
//! - [`SessionEndReason`] — passed to `close`.
//! - [`SessionSnapshot`] — returned by `snapshot`.
//! - [`manager::SessionRegistry`] — owns the per-session sink
//!   registry and the shared input queue. Server-side glue;
//!   agents don't talk to it directly.
//! - [`token_meter::TokenMeter`] — pure replay fold that turns
//!   the durable event stream into provider-anchored token
//!   pressure for compaction gating and status displays.
//!
//! ## Backend implementations
//!
//! - [`in_memory::InMemorySessionSink`] — testing only, no
//!   persistence.
//! - [`jsonl::JsonlSessionSink`] — production on-disk backend,
//!   one event per line, fsync'd on append.
//!
//! ## What used to live here and where it went
//!
//! | Old module | New home |
//! |---|---|
//! | `manager::*` (god object) | [`manager::SessionRegistry`] — minimal sink-registry façade; policy moved to server |
//! | `state_machine/*` | deleted; orchestration policy belongs to the server layer |
//! | `token_budget.rs` | deleted; token budgets are tracked inside the agent loop |
//! | `store/*` | reimplemented as [`jsonl::JsonlSessionSink`] |
//!
//! ## Dependency direction
//!
//! ```text
//! synthia-server  →  synthia-session  ←  synthia-harness
//! ```
//!
//! `synthia-session` is a leaf crate: it depends on `serde`,
//! `serde_json`, `async_trait`, `tokio` (for async-mutex +
//! `spawn_blocking` for fsync), and `parking_lot` — it does
//! **not** depend on `synthia-harness`. Callers (specifically the
//! agent loop) serialize their events to `serde_json::Value`
//! before calling `append`.

pub mod compaction_checkpoint;
pub mod events;
pub mod in_memory;
pub mod jsonl;
pub mod log_surface;
pub mod manager;
pub mod operation;
pub mod repair;
pub mod search;
pub mod sink;
pub mod surface;
pub mod token_meter;
pub mod tool_pairing;
pub mod typed_event_builders;
pub mod typed_event_sink;

pub use compaction_checkpoint::{
    CompactionCheckpoint,
    CompactionLifecycleView,
};
pub use events::{
    CompactionOutcome,
    KNOWN_SESSION_EVENT_TYPES,
    ReplaceRange,
    SessionEvent,
    SurfaceOp,
    SurfaceToken,
    empty_event_of,
};
pub use jsonl::{
    CURRENT_SCHEMA_VERSION,
    MetaMarker,
    SessionMetadataHeader,
    is_metadata_header_row,
};
pub use log_surface::{
    MappingGap,
    ResolvedReplace,
    SharedSurfaceLedger,
    SurfaceLedger,
    fold_log_surface,
    surface_events_from_log,
    try_fold_log_surface,
};
pub use operation::{
    CompactionRef,
    DEFAULT_BUS_CAPACITY,
    OperationSnapshot,
    OperationState,
    SnapshotBus,
    SnapshotReceiver,
};
pub use repair::{
    OrphanedCompaction,
    TOOL_NOT_STARTED,
    TOOL_OUTCOME_UNKNOWN,
    TURN_INTERRUPTED,
    compaction_outcomes,
    interrupted_turn_closers,
    orphaned_compactions,
};
pub use search::{
    DEFAULT_LIMIT,
    EntryHit,
    JsonlSessionSearch,
    SearchError,
    SearchQuery,
    SessionHit,
    SessionSearch,
    SharedSessionSearch,
    jsonl_session_search,
    modified_at,
};
pub use sink::{SessionEndReason, SessionError, SessionSink, SessionSnapshot};
pub use surface::{FoldError, FoldedSurface, assemble_chunks, fold_surface};
pub use token_meter::{
    ContextPressure,
    MeasurementBaseline,
    TokenMeasurement,
    TokenMeter,
    TokenMeterError,
    UsageBuckets,
    estimate_message_tokens,
};
pub use tool_pairing::{
    BalanceCache,
    UnbalancedCut,
    build_balance_cache,
    compaction_balanced,
    validate_replace_with_balance,
};
pub use typed_event_builders::{
    iteration_end,
    iteration_start,
    request_header,
    round_trip,
    step_end,
    step_start,
    structural_kind,
    subagent_enter,
    subagent_exit,
    turn_end,
    turn_start,
    usage,
};
pub use typed_event_sink::{
    CompactionRecordView,
    TypedEventReceiver,
    TypedEventRecord,
    TypedEventSink,
};
