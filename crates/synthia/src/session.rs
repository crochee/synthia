//! [`synthia_session`] — the durable session log.
//!
//! One trait ([`SessionSink`], five required methods plus two
//! default-implemented ones) with two backends
//! ([`in_memory::InMemorySessionSink`], [`jsonl::JsonlSessionSink`])
//! and the typed event
//! vocabulary that rides on it ([`SessionEvent`], [`TypedEventSink`] /
//! [`TypedEventReceiver`]). [`TokenMeter`] folds a persisted log back
//! into provider-anchored token pressure — the input to compaction
//! gating and status displays — without re-reading the conversation.

pub use synthia_session::*;
