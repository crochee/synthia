//! The single effect seam between a workflow and the world.
//!
//! Every effect a run has on the outside — spawning an agent, running a
//! gate — crosses [`WorkflowHost`]. The runtime therefore owns nothing
//! that cannot be faked: caps, admission, ordering, journaling and live
//! control are all decided on this side of the seam, and the tests drive
//! them with a host that does no work at all. The same property is what
//! makes the runtime runtime-neutral: there are no timers, tasks or
//! tokens in this crate's public API, only futures the caller drives.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{error::WorkflowError, spec::GateRef};

/// One agent call the runtime wants performed.
///
/// Everything a host needs to spawn and to attribute the result: the
/// run and step it belongs to, its position in the plan (the journal
/// index), the agent and prompt, and whether the call asked for
/// isolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRequest {
    /// Id of the run this call belongs to.
    pub run_id: String,
    /// Id of the step that planned the call.
    pub step_id: String,
    /// Position in the run's plan; the journal index of the call.
    pub position: usize,
    /// Pipeline nesting depth: 0 at the top level, one per enclosing
    /// pipeline.
    pub depth: usize,
    /// Agent to spawn.
    pub agent: String,
    /// The prompt, with a pipeline stage's chained input already
    /// appended when there was one.
    pub prompt: String,
    /// Gate the caller declared for this step, when it did.
    pub gate: Option<GateRef>,
    /// Whether the call asked to run in an isolated worktree.
    pub isolation: bool,
    /// Working directory the run was configured with, when it has one.
    pub cwd: Option<PathBuf>,
}

/// What one agent call produced.
///
/// A failure is data, not an error: the call settles with `error` set
/// and the document's remaining steps still run — that is what makes a
/// workflow of independent agents survivable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentOutcome {
    /// The agent's final text, when it produced one.
    pub text: Option<String>,
    /// Why the agent failed, when it did.
    pub error: Option<String>,
}

impl AgentOutcome {
    /// A successful call with `text` as its output.
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            error: None,
        }
    }

    /// A failed call. The message is journaled, so a resume re-runs it.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            text: None,
            error: Some(message.into()),
        }
    }

    /// Whether the call succeeded.
    pub fn succeeded(&self) -> bool {
        self.error.is_none()
    }
}

/// What a gate command did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GateOutcome {
    /// Whether the command exited zero.
    pub passed: bool,
    /// The command's output, shown when it did not pass.
    pub output: String,
}

impl GateOutcome {
    /// The command passed.
    pub fn passed() -> Self {
        Self {
            passed: true,
            output: String::new(),
        }
    }

    /// The command failed, with `output` as the explanation.
    pub fn failed(output: impl Into<String>) -> Self {
        Self {
            passed: false,
            output: output.into(),
        }
    }
}

/// How a step's gate treated one call.
///
/// The summary a run reports per call and a selector is told per
/// candidate: [`GateOutcome`] where the outcome itself is not available
/// any more. Every state is something the runtime actually knows — none
/// of them claims more than that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateVerdict {
    /// The step declared no gate, so there was nothing to pass.
    Absent,
    /// The gate command ran for this call and exited zero.
    Passed,
    /// The gate command ran for this call and did not exit zero. The
    /// call's error carries the command's output.
    Failed,
    /// The gate never judged this call: the agent failed before it
    /// could run, live control skipped the call, an abort cut it short,
    /// or the call came back from a journal entry, which records that a
    /// call failed but not which half of it did.
    Unobserved,
}

/// What one candidate of a selection produced.
///
/// A failure is data here, exactly as it is on [`AgentOutcome`]: a
/// candidate that failed is reported, not raised. Only a call that
/// settled as a success may win.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateOutcome {
    /// The agent answered, and the step's gate (when it declared one)
    /// passed.
    Succeeded {
        /// The agent's text; empty when it produced none.
        text: String,
    },
    /// The agent or its gate failed; the candidate's gate verdict says
    /// which.
    Failed {
        /// Why the candidate failed, as recorded for the call.
        error: String,
    },
    /// The candidate never settled: live control skipped it, or an
    /// abort cut the step short before it started.
    Unsettled,
}

/// One candidate of a `best_of` step, as a selector sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionCandidate {
    /// The prompt this candidate ran with, a pipeline stage's chained
    /// input already appended.
    pub prompt: String,
    /// What the candidate produced.
    pub outcome: CandidateOutcome,
    /// How the step's gate treated the candidate.
    pub gate: GateVerdict,
}

/// Everything a selector needs to choose a `best_of` step's winner.
///
/// Built by the runtime once the step's candidates have settled, and
/// handed to [`WorkflowHost::select_candidate`]. It is plain data —
/// no runtime types, no journal positions — so a judge (a rubric, a
/// scoring function, a model) can be written against it without knowing
/// how the run is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionRequest {
    /// Id of the run the step belongs to.
    pub run_id: String,
    /// The `best_of` step being decided.
    pub step_id: String,
    /// The agent every candidate ran under.
    pub agent: String,
    /// The step's gate command, when it declared one; each candidate's
    /// verdict says how it fared against it.
    pub gate: Option<GateRef>,
    /// One candidate per item, in the step's item order. The index
    /// [`WorkflowHost::select_candidate`] returns addresses this list.
    pub candidates: Vec<SelectionCandidate>,
}

/// The one seam between the workflow runtime and the rest of the world.
///
/// Implementations own the real work: the process, the model, the
/// timeout, the working directory. Returning `Err` means *the host could
/// not answer* and stops the run; returning
/// [`AgentOutcome::failed`] means *the agent answered with a failure*
/// and does not.
#[async_trait]
pub trait WorkflowHost: Send + Sync {
    /// Spawn one agent and wait for it to settle.
    ///
    /// # Errors
    ///
    /// Any [`WorkflowError`] that means the call could not be made at
    /// all.
    async fn spawn_agent(
        &self,
        request: &AgentRequest,
    ) -> Result<AgentOutcome, WorkflowError>;

    /// Run a step's gate command.
    ///
    /// `cwd` is the run's configured working directory, when it has one.
    ///
    /// # Errors
    ///
    /// Any [`WorkflowError`] that means the command could not be run.
    async fn run_gate(
        &self,
        gate: &GateRef,
        cwd: Option<&Path>,
    ) -> Result<GateOutcome, WorkflowError>;

    /// Choose a `best_of` step's winner.
    ///
    /// Called once for a selection this run actually ran candidates
    /// for. A step answered entirely from the journal is *not* offered
    /// here: it reuses the decision its first run recorded, so replay
    /// never re-asks a selector. The default implementation declines,
    /// which leaves the built-in rule in place: the first candidate
    /// that settled as a success, in item order — with a gate, the
    /// first candidate that passed it.
    ///
    /// `Ok(None)` means the same thing as not implementing this method:
    /// *use the built-in rule*. `Some(index)` addresses
    /// [`SelectionRequest::candidates`] and must name a candidate that
    /// settled as a success; a candidate that failed, failed its gate,
    /// was skipped, or was cut short by an abort cannot win, and an
    /// index outside the step cannot either. Both are rejected with
    /// [`WorkflowError::Selection`] rather than quietly falling back to
    /// the built-in rule: silently overriding a judge that answered
    /// wrongly would hide the bug behind a plausible-looking winner,
    /// while stopping costs nothing that has not already been paid —
    /// every candidate settled and was journaled before this call.
    ///
    /// Returning `Err` means *the host could not answer at all* and
    /// stops the run, like every other effect on this seam.
    ///
    /// # Errors
    ///
    /// Any [`WorkflowError`] that means the choice could not be made.
    async fn select_candidate(
        &self,
        request: &SelectionRequest,
    ) -> Result<Option<usize>, WorkflowError> {
        let _ = request;
        Ok(None)
    }
}

#[async_trait]
impl<T: WorkflowHost + ?Sized> WorkflowHost for std::sync::Arc<T> {
    async fn spawn_agent(
        &self,
        request: &AgentRequest,
    ) -> Result<AgentOutcome, WorkflowError> {
        (**self).spawn_agent(request).await
    }

    async fn run_gate(
        &self,
        gate: &GateRef,
        cwd: Option<&Path>,
    ) -> Result<GateOutcome, WorkflowError> {
        (**self).run_gate(gate, cwd).await
    }

    async fn select_candidate(
        &self,
        request: &SelectionRequest,
    ) -> Result<Option<usize>, WorkflowError> {
        (**self).select_candidate(request).await
    }
}
