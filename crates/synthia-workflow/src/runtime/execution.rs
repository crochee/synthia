//! How one attempt settles, and how one step's calls are shaped into
//! its result. Owned by the per-run `Executor`.

use crate::{
    host::{CandidateOutcome, GateVerdict},
    result::{CallRun, CallStatus},
    spec::GateRef,
};

/// Marker a pipeline inserts before the previous stage's text.
pub(super) const CHAIN_MARKER: &str = "--- output from the previous stage ---";

pub(super) enum Attempt {
    /// The call settled, successfully or not.
    Settled {
        text: Option<String>,
        error: Option<String>,
        /// How the step's gate treated the call.
        gate: GateVerdict,
    },
    /// Live control skipped the call before it started.
    Skipped,
    /// The run is aborted; the call must not start.
    Aborted,
    /// The run was paused after the call was admitted.
    Paused,
}

/// What one step produced.
pub(super) struct Execution {
    pub(super) calls: Vec<CallRun>,
    pub(super) output: Option<String>,
}

impl Execution {
    /// A step's result, with its chained output derived from its calls.
    pub(super) fn of_calls(calls: Vec<CallRun>) -> Self {
        let output = chain_output(&calls);
        Self { calls, output }
    }
}

/// A `best_of` step's result: the step's output is the candidate its
/// decision chose, and every other candidate that ran here is re-marked
/// as one that lost.
///
/// The decision is made and checked by the time this runs: `winner` is
/// the index of a call that settled as a success, or `None` when no
/// candidate can win — a selection no candidate passed, or one control
/// skipped or an abort cut short. Only what ran here is re-marked. A
/// candidate answered from the journal stays `Replayed` — it never
/// crossed the host, and `Replayed` is the status that says so; whether
/// the selection passed it over is reported by [`CallRun::winner`] — and
/// a candidate control skipped or an abort cut short keeps its status,
/// because those are facts about the run rather than about the
/// selection.
pub(super) fn selection(
    mut calls: Vec<CallRun>,
    winner: Option<usize>,
) -> Execution {
    let Some(winner) = winner.filter(|index| *index < calls.len()) else {
        // Nobody won: the step failed, and the failures it keeps are
        // exactly what the caller needs to read. Nothing is re-marked,
        // so every candidate reports how it died.
        return Execution {
            calls,
            output: None,
        };
    };
    calls[winner].select();
    // A winner that produced no text leaves the step without an output,
    // the same way a fan-out with nothing to say does.
    let output = calls[winner].text.clone().filter(|text| !text.is_empty());
    for (index, call) in calls.iter_mut().enumerate() {
        if index != winner
            && matches!(call.status, CallStatus::Succeeded | CallStatus::Failed)
        {
            call.supersede();
        }
    }
    Execution { calls, output }
}

/// The first candidate, in item order, that settled as a success: the
/// rule a selection falls back to whenever nothing else decides it — the
/// default host declining, a journal that carries no decision, or a
/// decision the host cannot make because control aborted the run.
pub(super) fn first_success(calls: &[CallRun]) -> Option<usize> {
    calls.iter().position(CallRun::succeeded)
}

/// How one call presents itself to a selector.
///
/// A call that never settled is not a failure: it has no error to
/// report, and a selector must not treat it as a candidate that ran and
/// died.
pub(super) fn candidate_outcome(call: &CallRun) -> CandidateOutcome {
    if call.succeeded() {
        CandidateOutcome::Succeeded {
            text: call.text.clone().unwrap_or_default(),
        }
    } else if matches!(call.status, CallStatus::Skipped | CallStatus::Aborted) {
        CandidateOutcome::Unsettled
    } else {
        CandidateOutcome::Failed {
            error: call.error.clone().unwrap_or_default(),
        }
    }
}

/// Why a candidate a host named cannot win its step.
///
/// A candidate that failed carries the reason it failed — including the
/// gate's own output when the gate is what failed it — so the caller
/// reads why the selector's answer was rejected, not just that it was.
pub(super) fn rejection_reason(call: &CallRun) -> String {
    match call.status {
        CallStatus::Skipped => "live control skipped it".to_owned(),
        CallStatus::Aborted => {
            "the run was aborted before it settled".to_owned()
        }
        CallStatus::Superseded => "it lost an earlier selection".to_owned(),
        _ => match call.error.as_deref() {
            Some(error) => format!("it did not succeed: {error}"),
            None => "it did not settle as a success".to_owned(),
        },
    }
}

/// The text a step hands to whatever follows it: every non-empty text it
/// produced, in position order, separated by a blank line.
fn chain_output(calls: &[CallRun]) -> Option<String> {
    let mut texts = Vec::new();
    for call in calls {
        if let Some(text) = call.text.as_deref()
            && !text.is_empty()
        {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n\n"))
    }
}

/// A pipeline stage's effective prompt: its own, with the previous
/// stage's text appended.
pub(super) fn chained_prompt(prompt: &str, previous: Option<&str>) -> String {
    match previous {
        Some(text) if !text.is_empty() => {
            format!("{prompt}\n\n{CHAIN_MARKER}\n{text}")
        }
        _ => prompt.to_owned(),
    }
}

pub(super) fn gate_failure(gate: &GateRef, output: &str) -> String {
    if output.trim().is_empty() {
        format!("gate `{}` failed", gate.command)
    } else {
        format!("gate `{}` failed:\n{output}", gate.command)
    }
}
