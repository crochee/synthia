//! The crate's single error type.
//!
//! The taxonomy separates the three moments a run can fail at:
//!
//! - **Planning** — [`WorkflowError::InvalidSpec`] and
//!   [`WorkflowError::CapExceeded`] are raised by
//!   [`WorkflowSpec::plan`](crate::WorkflowSpec::plan) before a single
//!   spawn has happened, so a bad document costs nothing.
//! - **The effect seam** — [`WorkflowError::Host`] and
//!   [`WorkflowError::Gate`] wrap whatever a [`WorkflowHost`] reported,
//!   with the step attached so the caller knows which effect died.
//!   [`WorkflowError::Selection`] is the one rejection that is not a
//!   wrapper: the host *did* answer, with a candidate its step cannot
//!   use, and the reason is spelled out instead of carried.
//! - **Workspace plumbing** — [`WorkflowError::Core`] lets a host that
//!   is already written against `synthia-core` use `?` instead of
//!   mapping every failure by hand.
//!
//! An agent that ran and *failed* is not an error: it is a settled
//! [`CallRun`](crate::CallRun) with
//! [`CallStatus::Failed`](crate::CallStatus::Failed) — or with
//! [`CallStatus::Superseded`](crate::CallStatus::Superseded), when it
//! failed as one candidate of a selection another candidate won — and a
//! skipped call is settled the same way: the document decides what to do
//! about it. Only a host that cannot answer at all stops the run.
//!
//! [`WorkflowHost`]: crate::WorkflowHost

use thiserror::Error;

/// Everything a workflow run can fail with.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WorkflowError {
    /// The document is not a runnable workflow: an empty step list, a
    /// duplicate step id, an empty prompt, a zero concurrency bound.
    #[error("invalid workflow spec: {message}")]
    InvalidSpec {
        /// What is wrong with the document.
        message: String,
    },

    /// The document could not be read or decoded.
    #[error("workflow document: {message}")]
    Document {
        /// The path (when there is one) and the underlying error.
        message: String,
    },

    /// The plan would exceed a [`WorkflowCaps`] limit.
    ///
    /// Raised while planning, so nothing has spawned yet.
    ///
    /// [`WorkflowCaps`]: crate::WorkflowCaps
    #[error("{cap} cap exceeded: limit {limit}, requested {requested}")]
    CapExceeded {
        /// Which cap tripped: `max_agents`, `max_items` or
        /// `max_nested`.
        cap: &'static str,
        /// The configured limit.
        limit: usize,
        /// What the document asks for.
        requested: usize,
    },

    /// The host failed to perform one of a step's effects.
    ///
    /// Raised when [`spawn_agent`](crate::WorkflowHost::spawn_agent) or
    /// [`select_candidate`](crate::WorkflowHost::select_candidate)
    /// reports a failure of its own: the host could not answer, so the
    /// step cannot settle at all.
    #[error("host failed running step `{step_id}`: {source}")]
    Host {
        /// The step whose effect failed.
        step_id: String,
        /// The failure reported by the host.
        #[source]
        source: Box<WorkflowError>,
    },

    /// The host failed to run a step's gate command.
    #[error("host failed running gate `{gate}` for step `{step_id}`: {source}")]
    Gate {
        /// The command that could not be run.
        gate: String,
        /// The step that declared the gate.
        step_id: String,
        /// The failure reported by the host.
        #[source]
        source: Box<WorkflowError>,
    },

    /// The host chose a candidate that cannot win its `best_of` step.
    ///
    /// Raised when
    /// [`select_candidate`](crate::WorkflowHost::select_candidate)
    /// answers with an index outside the step's candidates, or with one
    /// that did not settle as a success — a candidate that failed,
    /// failed its gate, was skipped, or was cut short by an abort. The
    /// run stops rather than falling back to the built-in rule: a
    /// selector that answered wrongly is a bug worth seeing, and every
    /// candidate has already settled and been journaled, so stopping
    /// wastes no work.
    #[error(
        "host selected candidate {selected} for step `{step_id}`, which cannot win: {reason}"
    )]
    Selection {
        /// The step whose selection was answered.
        step_id: String,
        /// The index the host returned.
        selected: usize,
        /// Why that candidate cannot be the step's winner.
        reason: String,
    },
    /// A `mcts` step names the `heuristic` scorer and the runtime has
    /// no registered scorer under that name.
    #[error(
        "mcts step `{step_id}` names a heuristic scorer but the runtime has none registered"
    )]
    MctsScorer {
        /// The step that asked for a heuristic scorer.
        step_id: String,
        /// The scorer the document named; only the heuristic variant
        /// is reported because that is the one the runtime has to
        /// resolve at run time.
        scorer: crate::scorer::MctsScorer,
    },

    /// A workspace error raised by a [`WorkflowHost`] implementation.
    ///
    /// [`WorkflowHost`]: crate::WorkflowHost
    #[error(transparent)]
    Core(#[from] synthia_core::Error),
}
