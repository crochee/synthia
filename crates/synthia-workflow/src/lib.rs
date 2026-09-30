//! Declarative multi-agent workflow runtime.
//!
//! A workflow is a **document**, not a script: [`WorkflowSpec`] is
//! serde round-trippable JSON describing ordered [`Step`]s — single
//! agents, fan-outs, selections and pipelines — with optional [`Phase`]
//! labels for reporting. [`WorkflowRuntime`] executes one against a
//! [`WorkflowHost`], which is the *only* way a run touches the world.
//!
//! That seam is what makes the rest of the crate enforceable: caps are
//! checked while planning, concurrency is a semaphore on this side,
//! aborting drains it, and every settled call is journaled so a later
//! run can resume instead of re-paying for work it already did.
//!
//! # Divergence from the reference implementation
//!
//! The reference (`pi-subagents/src/workflow/*.ts`) runs a JavaScript
//! script in a worker VM. The script decides the orchestration at run
//! time; the host can only constrain it from outside — caps on the calls
//! it makes, a semaphore around the spawns it requests, a journal keyed
//! by call arrival order. This crate keeps the host half (caps,
//! admission, prefix replay, live control) and replaces the script half
//! with a document. Four things change, all of them deliberate:
//!
//! - **Positions are pre-order, not arrival order.** The reference
//!   assigns a journal position when a call arrives, which under
//!   concurrency depends on who finished first; it survives that by
//!   checking the key as well as the position, and settles for losing
//!   cache hits when a run interleaves differently. A document has an
//!   order before it runs, so [`WorkflowPlan::calls`] fixes every
//!   position up front and a resume matches exactly.
//! - **Caps are planning errors, not runtime ones.** The reference
//!   counts calls as the script makes them and fails the call that
//!   crosses the line, after the ones before it were paid for. Here
//!   [`WorkflowError::CapExceeded`] is raised by
//!   [`WorkflowSpec::plan`] with nothing spawned.
//! - **A selection is judged by the host, and decided by the
//!   document's fallback.** The MCTS strategy [`Step::BestOf`] follows
//!   ranks finished branches with a scoring closure and takes the best.
//!   A closure is not data: it cannot round-trip through a document or
//!   replay from a journal. So the *document* keeps the only rule it can
//!   state — the first candidate that settled as a success, in item
//!   order — and a better judgement is asked of the host
//!   ([`WorkflowHost::select_candidate`]), which may decline and leave
//!   that rule in place. The decision is journaled like any other
//!   result, so a replayed run reproduces a host's choice instead of
//!   paying a judge twice.
//! - **The document cannot express what a script can.** Conversation
//!   continuation between calls (`resume`), per-call model and effort
//!   overrides, and result schemas are host concerns here: they are not
//!   effects the runtime needs to order, cap or journal. A host is free
//!   to implement them behind [`WorkflowHost::spawn_agent`].
//!
//! # Quick start
//!
//! ```
//! use synthia_workflow::{AgentStep, Step, WorkflowCaps, WorkflowSpec};
//!
//! let document = WorkflowSpec {
//!     id: "audit".to_owned(),
//!     steps: vec![Step::Agent(AgentStep {
//!         id: "scan".to_owned(),
//!         agent: "scout".to_owned(),
//!         prompt: "list the modules and their sizes".to_owned(),
//!         gate: None,
//!         isolation: false,
//!     })],
//!     phases: Vec::new(),
//! };
//!
//! let plan = document.plan(&WorkflowCaps::default())?;
//! assert_eq!(plan.call_count(), 1);
//! # Ok::<(), synthia_workflow::WorkflowError>(())
//! ```

#![deny(missing_docs)]

pub mod concurrency;
pub mod control;
pub mod error;
pub mod host;
pub mod journal;
pub mod plan;
pub mod result;
pub mod runtime;
pub mod scorer;
pub mod spec;
pub use crate::{
    concurrency::default_max_concurrency,
    control::WorkflowControl,
    error::WorkflowError,
    host::{
        AgentOutcome,
        AgentRequest,
        CandidateOutcome,
        GateOutcome,
        GateVerdict,
        SelectionCandidate,
        SelectionRequest,
        WorkflowHost,
    },
    journal::{
        JournalEntry,
        JournalKeyInput,
        append_journal,
        journal_key,
        read_journal,
    },
    plan::{
        DEFAULT_MAX_AGENTS,
        DEFAULT_MAX_ITEMS,
        DEFAULT_MAX_NESTED,
        PlannedCall,
        PlannedStep,
        WorkflowCaps,
        WorkflowPlan,
    },
    result::{CallRun, CallStatus, WorkflowRun},
    runtime::WorkflowRuntime,
    scorer::{
        BuiltinScorer,
        HeuristicScorer,
        MctsScorer,
        ScoredBranch,
        Scorer,
    },
    spec::{
        AgentStep,
        BestOfStep,
        FanOutStep,
        GateRef,
        MctsStep,
        Phase,
        PipelineStep,
        Step,
        WorkflowSpec,
    },
};
