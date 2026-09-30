//! The curated prelude: `use synthia::prelude::*;` — then assemble.
//!
//! Everything here is a re-export; the framework's zero-logic rule
//! applies to the prelude as much as to the modules. It is **not** the
//! whole surface: every crate's long tail stays reachable by module
//! path (`synthia::tool::GroupedRegistry`,
//! `synthia::session::SessionEvent`, `synthia::harness::AgentRegistry`,
//! …). If a name is not here, the module that owns it has it.
//!
//! - **The seven lego pieces of the assembly tutorial**: the traits
//!   [`Agent`], [`ModelProvider`], [`Tool`], [`ContextManager`],
//!   [`CancelToken`]; the two concrete pieces a basic agent needs
//!   ([`ReActAgent`], [`TruncatingContextManager`]); the registries
//!   and containers ([`ToolRegistry`], [`Steering`],
//!   [`TypedEventSink`], [`AtomicCancelToken`]).
//! - **The runtime-neutral time and id vocabularies**: the
//!   [`Clock`] trait plus [`SystemClock`] (production default)
//!   and [`FixedClock`] (tests); the [`IdGen`] trait plus
//!   [`UlidGenerator`] and [`SequenceGenerator`]; the
//!   `SharedClock` / `SharedIdGen` newtypes that hide the
//!   `Arc<dyn …>` boilerplate. Together they replace scattered
//!   `chrono::Utc::now` and `ulid::Generator::generate` calls
//!   with injectable seams — see
//!   [`synthia::core`](crate::core) for the rationale.
//! - **The assembly itself**: [`ReActAgent`] plus [`ToolEntry`],
//!   the registration wrapper for the plugin tool crates.
//! - **How a run is driven**: [`AgentInput`] in, [`AgentEvent`] out.
//! - **How a provider is written by hand**: the trait's signature
//!   vocabulary ([`CompletionRequest`], [`CompletionResponse`],
//!   [`Content`], [`Message`], [`ContentPart`], [`ModelConfig`],
//!   [`ProviderConfig`]) and [`macro@async_trait`], the attribute macro
//!   every async trait in the framework is implemented with. Without
//!   the macro a consumer would need a second dependency just to
//!   implement `ModelProvider` or `Tool` — the thing this facade
//!   exists to prevent.
//! - **The error vocabulary**: [`Error`] and [`Result`].
//!
//! # Name collision decisions
//!
//! A prelude is a glob, so every name in it can only mean one thing.
//! The three collisions that actually occur in this tree, and the
//! choice made for each:
//!
//! 1. **`Result`** — [`synthia_core::Result`] is the canonical alias
//!    (`Result<T, E = Error>`), so that is what `Result` means here.
//!    `synthia::tool::Result<T>` is deliberately *not* re-exported: it
//!    is the same shape with the error pinned
//!    (`std::result::Result<T, synthia_core::Error>`), so a
//!    `synthia::tool::Result<T>` value is accepted anywhere the
//!    prelude's two-parameter `Result` is, and re-exporting both would
//!    shadow one with the other for no gain.
//! 2. **`Tool`** — the **trait** (`synthia::tool::Tool`) is what
//!    `Tool` means here. The `#[derive(Tool)]` macro from
//!    `synthia-macros` shares the short name; it stays at
//!    [`crate::macros::Tool`], where a consumer who wants the derive
//!    can name it explicitly (a type and a macro live in different
//!    namespaces, but a glob prelude that imported both would make
//!    `use synthia::prelude::*` ambiguous at the call site).
//! 3. **`Context`** — [`Context`] is `synthia-tool`'s per-invocation
//!    execution context (session id, workspace root, truncation
//!    config): the type a [`Tool`] implementation receives. There is no
//!    competing `Context` in the provider crate in this revision —
//!    `synthia-provider`'s conversation types are [`Message`] /
//!    [`ContentPart`] / [`CompletionRequest`], and the context
//!    *manager* trait is [`ContextManager`]. Had both existed, tool's
//!    would still win: it is the one a tool author must name in a
//!    signature, and the manager trait has its own distinct name.
//!
//! A fourth name is a collision the prelude **excludes**: two
//! unrelated types called `SessionEndReason` exist —
//! `synthia::harness::SessionEndReason` (why a *run* ended: `Completed`
//! / `Cancelled` / `Error(String)` / `MaxIterations`) and
//! `synthia::session::SessionEndReason` (why a *sink* was closed:
//! `Completed` / `Cancelled` / `Error` / `Interrupted`). They are not
//! interchangeable, so neither is in the prelude; name the one you
//! mean by module path.
//!
//! `Error` has no collision: every crate's fallible API returns
//! [`synthia_core::Error`], including `synthia-tool`'s `Result` alias.
//!
//! # Feature gating
//!
//! Each item appears only when its piece's feature is enabled (see the
//! crate root's feature table). With `--no-default-features` the
//! prelude is empty and `use synthia::prelude::*;` still compiles —
//! the names are absent, not broken.

pub use async_trait::async_trait;

#[cfg(feature = "context")]
pub use crate::context::{ContextManager, TruncatingContextManager};
#[cfg(feature = "core")]
pub use crate::core::spawn::{SharedSpawner, Spawner};
#[cfg(feature = "core")]
pub use crate::core::{
    AtomicCancelToken,
    CancelToken,
    Clock,
    Error,
    FixedClock,
    IdGen,
    Result,
    SequenceGenerator,
    SharedClock,
    SharedIdGen,
    SystemClock,
    UlidGenerator,
};
#[cfg(feature = "harness")]
pub use crate::harness::{Agent, AgentEvent, AgentInput, ReActAgent};
#[cfg(feature = "provider")]
pub use crate::provider::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    Message,
    ModelConfig,
    ModelProvider,
    ProviderConfig,
};
#[cfg(feature = "session")]
pub use crate::session::TypedEventSink;
#[cfg(feature = "steering")]
pub use crate::steering::Steering;
#[cfg(feature = "tool")]
pub use crate::tool::{Context, Tool, ToolEntry, ToolOutput, ToolRegistry};
