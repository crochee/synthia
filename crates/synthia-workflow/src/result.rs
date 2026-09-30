//! What a run reports.
//!
//! A run always reports one [`CallRun`] per planned call, in position
//! order — including the calls an abort never reached — so the result
//! lines up with [`WorkflowPlan::calls`](crate::WorkflowPlan::calls) by
//! index and a caller can tell what happened to everything the document
//! asked for.

use serde::{Deserialize, Serialize};

use crate::{
    host::GateVerdict,
    journal::JournalEntry,
    plan::PlannedCall,
    runtime::SKIPPED_MESSAGE,
};

/// How an MCTS branch settled: which branch of which step it was, its
/// score, and the verdict that produced it.
///
/// One entry per branch's depth-`max_depth` call (the only call the
/// scorer reads); intermediate depths report `None`. Branches that did
/// not finish a single successful depth — every call failed or was
/// skipped — get a `score: None` and the runtime reports their failure
/// in [`CallRun::error`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchScore {
    /// Position of the branch's depth-0 call; the tie-break key.
    pub branch_id: usize,
    /// Branch index in the step's branch list, 0-based.
    pub branch_index: usize,
    /// Final text the scorer saw, when one was produced.
    pub text: Option<String>,
    /// Score the scorer returned for this branch.
    pub score: Option<f64>,
    /// Gate the branch's last call was scored under.
    pub gate: GateVerdict,
    /// Whether the step's scoring named this branch as the winner.
    pub winner: bool,
}

/// How one planned call settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallStatus {
    /// The host spawned an agent and it succeeded (and passed its gate,
    /// when it had one).
    Succeeded,
    /// The call was answered from the journal instead of spawned.
    ///
    /// A `best_of` candidate that lost may be answered this way from an
    /// entry that recorded a failure: the run then reports that failure
    /// and treats the call as a loser, not as a success. A candidate
    /// that lost keeps this status even when it recorded a success,
    /// because `Replayed` is what says it never crossed the host in
    /// this run; [`CallRun::winner`] is what says the selection passed
    /// it over.
    Replayed,
    /// The agent failed, or its gate did.
    Failed,
    /// The call ran and lost its `best_of` step's selection: another
    /// candidate won. Whatever it produced — text or error — is kept,
    /// and it never makes the run look failed, because the step it
    /// belongs to has a winner.
    Superseded,
    /// Live control skipped the call.
    Skipped,
    /// The run was aborted before the call could be admitted.
    Aborted,
}

impl CallStatus {
    /// Whether the run spawned an agent for this call.
    ///
    /// A superseded call ran (a call that never started cannot lose a
    /// selection), and an entry reused from the journal is `Replayed`
    /// whatever it recorded, so this stays an exact count of the calls
    /// that crossed the host in this run.
    pub fn crossed_host(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Superseded)
    }
}

/// What happened to one planned call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallRun {
    /// Position in the plan; the same index the journal uses.
    pub position: usize,
    /// The step that planned the call.
    pub step_id: String,
    /// The phase label covering the step, when a phase named it.
    pub phase: Option<String>,
    /// How the call settled.
    pub status: CallStatus,
    /// The agent's text, when it produced one.
    pub text: Option<String>,
    /// Why the call failed, when it did.
    pub error: Option<String>,
    /// How the step's gate treated the call.
    pub gate: GateVerdict,
    /// Whether the step's selection chose this call.
    ///
    /// Set on the winner of a `best_of` step — whether it ran here or
    /// came back from the journal — for the same reason the journal
    /// records it: with a host deciding, the winner is no longer
    /// derivable from the statuses alone, and
    /// [`WorkflowRun::winner_of`] has to report the call the step's
    /// decision actually chose. False for every call of a step that
    /// selects nothing, and for the candidates a selection did not
    /// choose.
    pub winner: bool,
    /// The branch score attached to this call, when it is an MCTS
    /// branch's depth-`max_depth` call. `None` for every other call —
    /// non-MCTS calls and MCTS calls that are not the branch's last.
    pub branch_score: Option<BranchScore>,
}

impl CallRun {
    fn new(
        call: &PlannedCall,
        status: CallStatus,
        text: Option<String>,
        error: Option<String>,
        gate: GateVerdict,
    ) -> Self {
        Self {
            position: call.position,
            step_id: call.step_id.clone(),
            phase: call.phase.clone(),
            status,
            text,
            error,
            gate,
            winner: false,
            branch_score: None,
        }
    }

    pub(crate) fn ok(
        call: &PlannedCall,
        text: String,
        gate: GateVerdict,
    ) -> Self {
        Self::new(call, CallStatus::Succeeded, Some(text), None, gate)
    }

    pub(crate) fn replayed(call: &PlannedCall, entry: &JournalEntry) -> Self {
        Self::new(
            call,
            CallStatus::Replayed,
            entry.text.clone(),
            entry.error.clone(),
            replayed_verdict(call, entry),
        )
    }

    pub(crate) fn failed(
        call: &PlannedCall,
        message: String,
        gate: GateVerdict,
    ) -> Self {
        Self::new(call, CallStatus::Failed, None, Some(message), gate)
    }

    pub(crate) fn skipped(call: &PlannedCall) -> Self {
        Self::new(
            call,
            CallStatus::Skipped,
            None,
            Some(SKIPPED_MESSAGE.to_owned()),
            GateVerdict::Unobserved,
        )
    }

    pub(crate) fn aborted(call: &PlannedCall) -> Self {
        Self::new(
            call,
            CallStatus::Aborted,
            None,
            None,
            GateVerdict::Unobserved,
        )
    }

    /// Mark a settled call as one that lost its step's selection.
    pub(crate) fn supersede(&mut self) {
        self.status = CallStatus::Superseded;
    }

    /// Mark a settled call as the one its step's selection chose.
    pub(crate) fn select(&mut self) {
        self.winner = true;
    }

    /// Whether this call settled as a success a step can build on: it
    /// spawned and succeeded, or came back from a journal entry that
    /// recorded a success.
    ///
    /// A `best_of` candidate reused from a *failed* entry is `Replayed`
    /// with `error` set. It did not succeed, so it cannot be the step's
    /// winner — and it does not sink the step either, because the journal
    /// entry it came from is a loss the step's winner already decided.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        matches!(self.status, CallStatus::Succeeded | CallStatus::Replayed)
            && self.error.is_none()
    }
}

/// The outcome of one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// Id of the run (`wf_` + a ULID); every request it made carries it.
    pub run_id: String,
    /// The last top-level step's text output, when it produced one.
    ///
    /// For a fan-out or pipeline that is every non-empty text the step
    /// produced, in position order, separated by a blank line. For a
    /// `best_of` step it is its winner's text alone: the candidates
    /// that lost report theirs in [`CallRun::text`], but the step does
    /// not hand it on.
    pub output: Option<String>,
    /// One entry per planned call, in position order.
    pub calls: Vec<CallRun>,
    /// How many calls came back from the journal instead of spawning.
    ///
    /// A candidate reused from an entry that recorded a failure is
    /// counted here too: it did not spawn in this run.
    pub replayed: usize,
    /// How many calls crossed the host in this run.
    pub spawned: usize,
}

impl WorkflowRun {
    /// Whether the run was aborted before it finished its document.
    ///
    /// True when the abort landed while there was still work to admit;
    /// an abort that arrives after the last call settled changes
    /// nothing.
    pub fn aborted(&self) -> bool {
        self.calls
            .iter()
            .any(|call| call.status == CallStatus::Aborted)
    }

    /// Whether every call settled as a success or a replay, or lost a
    /// `best_of` selection to one.
    ///
    /// A candidate that lost does not count against the run: the step it
    /// belongs to passed, and only a step no candidate won leaves
    /// failures behind. A skipped or aborted call does count: control
    /// changed the run, which is not the same as losing a selection.
    pub fn succeeded(&self) -> bool {
        self.calls.iter().all(|call| {
            matches!(
                call.status,
                CallStatus::Succeeded
                    | CallStatus::Replayed
                    | CallStatus::Superseded
            )
        })
    }

    /// The call a step reports as its output: the candidate its
    /// `best_of` selection chose, or the first call that succeeded.
    ///
    /// `None` when the step has no successful call — a `best_of` step no
    /// candidate won, or a step whose calls all failed or were skipped.
    /// Step ids are unique to one step, so the call this returns is
    /// unambiguous.
    ///
    /// A `best_of` step is found through the decision its run recorded
    /// ([`CallRun::winner`]), not by re-deriving the rule: a host may
    /// pick a later candidate, and on a replayed run the candidates
    /// before that one are reused successes which did not win. For a
    /// step that selects nothing it is simply that step's first
    /// success.
    #[must_use]
    pub fn winner_of(&self, step_id: &str) -> Option<&CallRun> {
        self.calls
            .iter()
            .find(|call| call.step_id == step_id && call.winner)
            .or_else(|| {
                self.calls
                    .iter()
                    .find(|call| call.step_id == step_id && call.succeeded())
            })
    }
}

/// How the gate treated a call that came back from the journal.
///
/// A journal entry records that a call settled as a success or as a
/// failure, not which half of it failed. A success under a declared
/// gate *must* have passed that gate — that is what made it a success —
/// so `Passed` is recoverable; a failure is not, because the agent may
/// have died before the gate ever ran.
fn replayed_verdict(call: &PlannedCall, entry: &JournalEntry) -> GateVerdict {
    match (call.gate.is_some(), entry.ok) {
        (false, _) => GateVerdict::Absent,
        (true, true) => GateVerdict::Passed,
        (true, false) => GateVerdict::Unobserved,
    }
}
