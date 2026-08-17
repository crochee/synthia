//! `synthia-session` — durable session storage for the agent runtime.
//!
//! ## Public surface (post-refactor)
//!
//! This crate used to expose ~30 types (`SessionManager`,
//! `Session`, `SessionStateMachine`, `Store`, …) plus state
//! machine, token budget, and approval modules. After the
//! refactor the public surface collapses to:
//!
//! - [`SessionSink`] — the single trait every consumer
//!   (typically `synthia-agent`) depends on. Exactly **5
//!   methods**: `id`, `append`, `read`, `snapshot`, `close`.
//! - [`SessionError`] — the only error type a sink returns.
//! - [`SessionEndReason`] — passed to `close`.
//! - [`SessionSnapshot`] — returned by `snapshot`.
//! - [`manager::SessionRegistry`] — owns the per-session sink
//!   registry and the shared input queue. Server-side glue;
//!   agents don't talk to it directly.
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
//! synthia-server  →  synthia-session  ←  synthia-agent
//! ```
//!
//! `synthia-session` is a leaf crate: it depends on `serde`,
//! `serde_json`, `async_trait`, `tokio` (for async-mutex +
//! `spawn_blocking` for fsync), and `parking_lot` — it does
//! **not** depend on `synthia-agent`. Callers (specifically the
//! agent loop) serialize their events to `serde_json::Value`
//! before calling `append`.

pub mod in_memory;
pub mod jsonl;
pub mod manager;
pub mod sink;

pub use sink::{SessionEndReason, SessionError, SessionSink, SessionSnapshot};
