//! The unified [`Agent`] runtime contract + the pieces assembled around
//! it.
//!
//! ## Layering
//!
//! Everything here is one of six things, and the modules say which:
//!
//! | Layer | Module | Concern |
//! |---|---|---|
//! | Contract | this file | [`Agent`] — the async trait every paradigm implements |
//! | Loop | `re_act` | [`ReActAgent`] + the ReAct strategy: one self-contained implementation |
//! | Reasoning | `strategy` | The seam that makes the loop a *choice*, plus the three strategies the crate ships |
//! | Driving a run | `run` | [`AgentHandle`] (run control: block on a run, or defer it to a handle) and [`RunInbox`] (messages into a live run) |
//! | Catalog | `descriptor`, `registry` | Agent identity + the multi-agent catalog |
//! | Plugin seam | `interceptor` | [`ToolInterceptor`] — synthetic tools that need loop internals |
//!
//! The submodules behind those layers are private: each one re-exports
//! the names it is *for*, so a reader can tell public API from internal
//! structure, and adding an item to an internal module does not silently
//! widen the crate's contract.
//!
//! ## What is deliberately *not* here
//!
//! Sub-agent delegation is a plugin. The `task` tool, its gates, worktree
//! isolation, the `@agent:` routers, and the subagent admission pool live
//! in the `synthia-tool-task` crate, which installs through
//! [`ToolInterceptor`]. With no interceptor installed the loop has no
//! delegation surface at all — there is no `task` branch of its own to
//! remove. What stays here is the part delegation *consumes*: the
//! [`AgentRegistry`] catalog, the loop, and the event types a child run's
//! traces are wrapped in ([`AgentEvent::Agent`] +
//! [`crate::AgentMeta`]).

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use futures::Stream;
use synthia_core::{CancelToken, registry::RegistryItem};

use crate::{events::AgentEvent, input::AgentInput};

mod descriptor;
mod interceptor;
mod re_act;
mod registry;
mod run;
mod spawn;
mod strategy;

pub use descriptor::{AgentDescriptor, AgentEntry, AgentFilter};
pub use interceptor::{InterceptorCall, ToolInterceptor};
pub use re_act::{
    DEFAULT_MAX_ITERATIONS,
    DEFAULT_SYSTEM_PROMPT,
    ReActAgent,
    ReActStrategy,
};
pub use registry::AgentRegistry;
pub use run::{
    AgentHandle,
    DetachedAgent,
    DetachedClosed,
    DetachedError,
    MpscInbox,
    RunInbox,
    RunInboxHandle,
};
pub use strategy::{
    AgentRuntime,
    BestOfNStrategy,
    CandidateScorer,
    ChainOfThoughtStrategy,
    DEFAULT_JUDGE_PROMPT,
    EventSink,
    KNOWN_STRATEGY_NAMES,
    LlmJudgeScorer,
    LongestAnswer,
    ReasoningStrategy,
    from_name,
};

/// A unified, asynchronous agent runtime contract.
///
/// Every concrete agent paradigm (ReAct, pipeline, planner,
/// router, …) implements this trait. The contract is deliberately
/// minimal so new paradigms can be added without changing the
/// call sites that drive them.
#[async_trait]
pub trait Agent: RegistryItem + Send + Sync {
    /// The agent's descriptor — name, instructions, capabilities.
    /// Cheap clone (`Arc` inside) so callers can introspect
    /// without taking ownership.
    fn descriptor(&self) -> &AgentDescriptor;

    /// Effective per-run iteration cap after clamping. Default
    /// implementation reads the descriptor's `max_iterations`
    /// hint and falls back to [`DEFAULT_MAX_ITERATIONS`]. Concrete
    /// agents with their own `with_max_iterations` builder
    /// override this so the HTTP layer can echo the **actually
    /// applied** cap (after `[1, 4096]` clamping) back to the
    /// client.
    fn effective_max_iterations(&self) -> usize {
        self.descriptor()
            .max_iterations
            .unwrap_or(DEFAULT_MAX_ITERATIONS)
    }

    /// Run one session and stream every [`AgentEvent`] in real
    /// time.
    ///
    /// `cancel` is wrapped in `Arc` so the returned stream and any
    /// sub-tasks spawned during execution can observe cancellation
    /// without further plumbing. The token is **not** consumed by
    /// this call — the caller retains ownership.
    ///
    /// Errors are surfaced *through the stream* as
    /// [`AgentEvent::System`] variants
    /// (`SessionEnded { reason: Error(..) }` for fatal errors,
    /// `Warning { .. }` for recoverable issues). The stream itself
    /// never yields `Err(_)` — `AgentEvent` is the only stream
    /// item. Unrecoverable internal panics propagate to the
    /// consumer via standard Rust panic semantics (the
    /// surrounding `tokio::spawn` join handle or `Stream::poll_next`
    /// caller); they are **not** silently swallowed. Fatal agent
    /// errors (provider failure, malformed streaming chunks, etc.)
    /// are converted to a terminal `SessionEnded { reason: Error(..) }`
    /// event before the stream closes.
    async fn run(
        &self,
        input: AgentInput,
        cancel: Arc<dyn CancelToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>;
}
