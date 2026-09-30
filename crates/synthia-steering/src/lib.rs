//! # synthia-steering
//!
//! The steering layer of the Synthia agent runtime: the traits
//! and built-ins that observe, nudge, and veto an agent run
//! without the agent loop knowing any concrete policy.
//!
//! Adopted from the TraitClaw trait vocabulary and the dsh
//! (deepseek-harness) interception semantics, with traitclaw's
//! own wiring defects deliberately fixed:
//!
//! | traitclaw defect | synthia fix |
//! |---|---|
//! | `Tracker::on_tool_call` never invoked | invoked at every dispatch boundary |
//! | guard state (budget/loop counters) held in interior-mutability shared across runs | per-run [`synthia_context::AgentState`] is the single source of truth |
//! | `Sanitize` result silently discarded | sanitized action is adopted before dispatch |
//! | `inject_hints` ignores `InjectionPoint`/priority | all four injection points + priority gating honoured |
//! | severity never consumed | severity rides the denial message + `WarningKind::Guard` event |
//!
//! ## Layering
//!
//! ```text
//! agent loop (single writer of AgentState)
//!   ├─ hooks    async lifecycle observation, tool veto (before guards)
//!   ├─ hints    advisory [reminder] injections (before each LLM call)
//!   ├─ tracker  sync observation + concurrency recommendation
//!   ├─ guards   sync fail-closed policy pipeline (per tool call)
//!   └─ output transformer  async tool-output rewrite (commit path)
//! ```
//!
//! Errors-as-results contract: a guard denial or hook block is
//! surfaced to the model as the tool result (an error string with
//! actionable coaching), never as a run abort — the model adapts,
//! the session survives.

pub mod action;
pub mod effect_gate;
pub mod guard;
pub mod guards;
pub mod hint;
pub mod hook;
pub mod hook_map;
pub mod output_transformer;
pub mod steering;
pub mod tier;
pub mod tracker;
pub mod transformer;
pub use action::{Action, tool_fingerprint};
pub use effect_gate::{AdmissionError, Gate, GateControl, GateState};
pub use guard::{
    Guard,
    GuardOutcome,
    GuardResult,
    GuardSeverity,
    NoopGuard,
    run_guards,
};
pub use guards::{
    CATASTROPHIC_SHELL_PATTERNS,
    LoopDetectionGuard,
    PromptInjectionGuard,
    ShellDenyGuard,
    ToolBudgetGuard,
    WorkspaceBoundaryGuard,
};
pub use hint::{
    ContextBudgetHint,
    Hint,
    HintMessage,
    HintPriority,
    InjectionPoint,
    IterationReminderHint,
    NoopHint,
    TruncationHint,
};
pub use hook::{
    AgentHook,
    HookAction,
    HookError,
    HookFuture,
    HookStage,
    LoggingHook,
    run_hook,
    run_hook_gated,
};
pub use hook_map::{
    HookDecision,
    HookEvent,
    HookHandler,
    HookHandlerWithState,
    HookMap,
    event_name,
};
pub use output_transformer::{NoopOutputTransformer, OutputTransformer};
pub use steering::Steering;
pub use tier::{auto, for_tier};
pub use transformer::{BudgetAwareTruncator, JsonExtractor, TransformerChain};
