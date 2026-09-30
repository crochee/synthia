//! [`synthia_core`] — the vocabulary every other piece
//! shares: the crate-wide [`Result`] alias and [`Error`] type, the
//! [`CancelToken`] trait (plus the std-only [`AtomicCancelToken`]),
//! the [`Registry`] trait, [`Sensitive`] secret handling, JSON-Schema
//! validation, full-output storage, text helpers, and the
//! runtime-neutral [`Clock`] and
//! [`IdGen`] abstractions.
//!
//! `synthia-core` has no runtime dependency: no tokio, no timers, no
//! HTTP. Everything below it in the facade assumes this crate's
//! error, cancellation, time, and id vocabulary.
//!
//! # Time and IDs: the standard pairing
//!
//! Every piece of Synthia that stamps a wall-clock instant or mints
//! an identifier takes a `SharedClock` / `SharedIdGen` rather than
//! calling `chrono::Utc::now()` or `ulid::Generator::generate()`
//! directly. The reason is the same as for [`CancelToken`]: tests
//! need a deterministic, injectable source; a host may want a
//! custom scheme. [`SharedClock::system`](synthia_core::SharedClock::system)
//! and [`SharedIdGen::ulid`](synthia_core::SharedIdGen::ulid) are
//! the production defaults.
//!
//! A consumer that wants timestamps only ever calls
//! `SharedClock::system()` once and passes it down; the same
//! instance can be shared across every component (clones share the
//! inner `Arc`).
// `synthia-core` itself is already curated (every public type
// is named explicitly in its lib.rs, no `pub use *`); the facade
// therefore propagates that curation verbatim. Internal helpers
// are `pub(crate)` in core and never reach this module — see
// `AGENTS.md §3.7` for the "公开面最小化" rule that keeps them out.
pub use synthia_core::*;
