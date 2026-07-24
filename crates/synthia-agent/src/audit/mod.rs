//! Audit logging — security-relevant event recording.
//!
//! Records permission grants/denials, blocked inputs/outputs,
//! credential redactions, loop-detection hits, circuit-breaker
//! triggers, agent lifecycle events, and hook errors to a
//! per-session JSONL file under `<workspace>/.synthia/audit.log`.
//!
//! # Module Layout
//!
//! - [`event_type`]: The [`event_type::AuditEventType`] enum
//!   (10 variants) + its `Display` impl that emits the
//!   snake_case wire string.
//! - [`severity`]: The [`severity::AuditSeverity`] enum
//!   (4 levels: Info/Warning/Error/Critical) + its `Display` impl.
//! - [`entry`]: The [`entry::AuditEntry`] struct + the
//!   [`entry::AuditEntry::new`] constructor that auto-fills the
//!   RFC 3339 timestamp.
//! - [`file_writer`]: The [`file_writer::FileAuditWriter`]
//!   inherent (non-trait) writer for ad-hoc single-entry writes.
//!   The `AuditWriter` trait that previously abstracted over
//!   backends was removed on 2026-06-15 in change
//!   `2026-06-15-p2-trait-cleanup` because it had 0 trait-bound
//!   usage, 0 dyn dispatch, and exactly 1 real implementation.
//! - [`logger`]: The [`logger::AuditLogger`] struct itself —
//!   the buffered logger that drains to disk at `max_buffer_size`
//!   (default 100) or on explicit `flush()`. Contains the
//!   7 typed helper methods
//!   ([`logger::AuditLogger::log_permission_granted`],
//!   [`logger::AuditLogger::log_permission_denied`],
//!   [`logger::AuditLogger::log_input_blocked`],
//!   [`logger::AuditLogger::log_output_blocked`],
//!   [`logger::AuditLogger::log_credential_redacted`],
//!   [`logger::AuditLogger::log_loop_detected`],
//!   [`logger::AuditLogger::log_circuit_breaker`]).
//! - [`tests`]: All 14 unit tests covering the directory-creation
//!   path, every typed helper, auto-flush at capacity, multi-line
//!   serialization, the empty-flush no-op, and the
//!   `FileAuditWriter` ad-hoc path.

mod entry;
mod event_type;
mod file_writer;
mod logger;
mod severity;

#[cfg(test)]
mod tests;

pub use entry::AuditEntry;
pub use event_type::AuditEventType;
pub use file_writer::FileAuditWriter;
pub use logger::AuditLogger;
pub use severity::AuditSeverity;
