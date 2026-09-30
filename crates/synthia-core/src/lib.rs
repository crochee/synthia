//! # synthia-core
//!
//! The foundation crate. Every other `synthia-*` crate depends on
//! this one and only this one — there is no upward call back.
//!
//! ## What this crate owns
//!
//! Each module is one **primitive** the rest of the framework cannot
//! reach without; together they are the smallest surface a Synthia
//! consumer can rely on.
//!
//! | Module | Primitive | Why it has to live here |
//! |---|---|---|
//! | [`error`] | [`Error`] — the single domain enum every fallible API returns | avoids `Box<dyn Error>` at every call site; one wire code (`Error::kind()`) |
//! | [`cancel`] | [`CancelToken`] trait + std-only [`AtomicCancelToken`] | runtime-neutral cancellation; tokio `CancellationToken` adapts without exposing runtime types in a lib signature |
//! | [`spawn`] | [`Spawner`](spawn::Spawner) trait — detached work, no runtime in the signature | the agent loop and tool dispatch detach work; naming `tokio::spawn` there would make the whole framework tokio-only |
//! | [`clock`] | [`Clock`] trait + [`SystemClock`] / [`FixedClock`] + [`SharedClock`] | chrono `DateTime<Utc>` is the wall-clock vocabulary; the trait is the testability seam |
//! | [`idgen`] | [`IdGen`] trait + [`UlidGenerator`] / [`SequenceGenerator`] + [`SharedIdGen`] | session / event / tool-call IDs under one injectable seam |
//! | [`agent`] | [`AgentDescriptor`] / [`AgentFilter`] — industry-aligned agent identity metadata | cross-crate surface (registry filters, delegation peers, server routes, eval harnesses) names agent identity without pulling the agent runtime in |
//! | [`registry`] | [`Registry`] / [`RegistryItem`] — generic catalog with cursor pagination, [`MAX_LIMIT`] = 100 | every "named catalog" in the workspace (agents, tools, skills, memories) plugs into one trait; the page-size cap is the single source of truth the server's wire-level limit aliases to |
//! | [`cursor`] | [`cursor::encode`] / [`cursor::decode`] — opaque URL-safe base64 cursor codec | the wire-level pagination primitive; `synthia-server` and the in-registry pagination use the same codec, so a cursor emitted by one round-trips on the other |
//! | [`text`] | UTF-8 safe [`cap_to_char_boundary`], [`truncate_chars`] | string truncation must never split a code point, and the char-based truncator is the single workspace primitive |
//! | [`token`] | cheap 4-char / 1.5-char (CJK) token estimator | pre-flight budget checks without a provider round-trip |
//! | [`sensitive`] | [`Sensitive`] / [`SensitiveData`] redacting newtype, [`redact_partial`] for display | secret-bearing values stay out of `Debug` logs; the partial-redaction helper lets a log show `sk-1***def` for an API key without leaking it |
//! | [`schema`] | JSON Schema validation helper | structured-output validation lives next to the wire types |
//! | [`full_output`] | [`FullOutputStore`] (chunked full-output retention) | replaces eager string concatenation in long tool results |
//! | [`judge_score`] | [`parse_judge_score`] | shared LLM-judge reply parser — `LlmJudgeMetric` and `LlmJudgeScorer` cannot drift |
//!
//! ## What's deliberately **not** here
//!
//! - **No provider wire types** (`Message`, `ContentPart`, `ModelConfig`)
//!   — those live in [`synthia_provider`] so a consumer that pulls
//!   `synthia-core` does not transitively depend on a model SDK.
//! - **No async runtime** — every primitive compiles on stable with no
//!   `tokio` / `async-std` / `smol` in the public API. The only async
//!   surface is the cooperative-cancellation future on
//!   [`CancelToken::cancelled`], which is hand-written and runtime
//!   agnostic.
//! - **No agent loop, tool, steering, memory, or session logic** —
//!   those belong in their own crates so each can be removed
//!   independently. A consumer that wants just the cancellation
//!   vocabulary can `cargo add synthia-core` and nothing else.
//!
//! ## Minimal MVP
//!
//! The smallest set a Synthia consumer ever imports from this crate:
//!
//! ```rust,ignore
//! use synthia_core::{AtomicCancelToken, CancelToken, Error, Result};
//! ```
//!
//! [`synthia_provider`]: ../synthia_provider/index.html

pub mod agent;
pub mod cancel;
pub mod clock;
pub mod cursor;
pub mod error;
pub mod full_output;
pub mod idgen;
pub mod judge_score;
pub mod panic;
pub mod registry;
pub mod schema;
pub mod sensitive;
pub mod spawn;
pub mod text;
pub mod token;

/// Crate-wide [`Result`] alias defaulting to the single error
/// type [`Error`]. Callers returning a different error type can
/// still override the parameter (`synthia_core::Result<T, E>`),
pub use agent::{AgentDescriptor, AgentFilter};
pub use cancel::{AtomicCancelToken, CancelToken};
pub use clock::{Clock, FixedClock, SharedClock, SystemClock};
pub use idgen::{IdGen, SequenceGenerator, SharedIdGen, UlidGenerator};
pub type Result<T, E = Error> = core::result::Result<T, E>;
pub use full_output::{FullOutputStore, InMemoryFullOutputStore};
pub use judge_score::parse_judge_score;
pub use panic::panic_message;
pub use registry::{
    MAX_LIMIT,
    Registry,
    RegistryItem,
    RegistryList,
    paginate_registry_list,
};
pub use schema::{SchemaViolation, validate_against_schema};
pub use sensitive::{
    Sensitive,
    SensitiveData,
    redact_partial,
    redact_partial_with,
};
pub use text::{cap_to_char_boundary, truncate_chars};

pub use crate::error::Error;
