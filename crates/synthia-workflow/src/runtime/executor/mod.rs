//! The per-run half of the runtime: one semaphore, one plan, one
//! journal. [`WorkflowRuntime`](super::WorkflowRuntime) builds one per
//! `run` and hands it the plan to walk.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use futures::future::{Either, join_all, select};

use super::{
    SKIPPED_MESSAGE,
    execution::{
        Attempt,
        Execution,
        candidate_outcome,
        chained_prompt,
        first_success,
        gate_failure,
        rejection_reason,
        selection,
    },
};
use crate::{
    concurrency::Semaphore,
    control::WorkflowControl,
    error::WorkflowError,
    host::{
        AgentOutcome,
        AgentRequest,
        GateVerdict,
        SelectionCandidate,
        SelectionRequest,
        WorkflowHost,
    },
    journal::{JournalEntry, append_journal},
    plan::{PlannedCall, PlannedStep},
    result::CallRun,
    scorer::{BuiltinScorer, BuiltinScorerKind, Scorer},
};

mod mcts;

pub(super) struct Executor<'a, H> {
    pub(super) host: &'a H,
    pub(super) control: &'a WorkflowControl,
    pub(super) run_id: &'a str,
    pub(super) cwd: Option<&'a Path>,
    pub(super) journal_path: Option<&'a Path>,
    pub(super) semaphore: Semaphore,
    pub(super) plan: &'a crate::plan::WorkflowPlan,
    pub(super) replay: &'a BTreeMap<usize, JournalEntry>,
    /// Heuristic scorer the runtime exposes for documents that name
    /// the `heuristic` scorer on the wire.
    pub(super) heuristic_scorer: Option<&'a Arc<dyn Scorer>>,
}

impl<H: WorkflowHost> Executor<'_, H> {
    /// Execute the plan, draining admission the moment the run is
    /// aborted.
    pub(super) async fn drive(&self) -> Result<Execution, WorkflowError> {
        let mut work = Box::pin(self.execute());
        let mut drain = Box::pin(async {
            self.control.wait_for_abort().await;
            self.semaphore.drain();
        });
        match select(work.as_mut(), drain.as_mut()).await {
            Either::Left((execution, _)) => execution,
            // Aborted: admission is drained, so stopping the walk cannot
            // strand a queued call. Permits already handed out are
            // awaited to completion before the run reports.
            Either::Right(((), _)) => work.as_mut().await,
        }
    }

    async fn execute(&self) -> Result<Execution, WorkflowError> {
        let mut calls = Vec::with_capacity(self.plan.call_count());
        let mut output = None;
        for step in self.plan.steps() {
            if self.control.is_aborted() {
                break;
            }
            let executed = self.run_step(step, None).await?;
            calls.extend(executed.calls);
            output = executed.output;
        }
        Ok(Execution { calls, output })
    }

    async fn run_step(
        &self,
        step: &PlannedStep,
        input: Option<&str>,
    ) -> Result<Execution, WorkflowError> {
        match step {
            PlannedStep::Agent(index) => {
                let call = self.call(*index);
                let run = self.run_call(call, input).await?;
                Ok(Execution::of_calls(vec![run]))
            }
            PlannedStep::FanOut {
                items, concurrency, ..
            } => {
                let calls = self.run_items(items, *concurrency, input).await?;
                Ok(Execution::of_calls(calls))
            }
            PlannedStep::BestOf {
                id,
                items,
                concurrency,
                ..
            } => {
                let calls = self.run_items(items, *concurrency, input).await?;
                self.select(id, items, input, calls).await
            }
            PlannedStep::Pipeline { stages, .. } => {
                let mut calls = Vec::new();
                let mut output = input.map(str::to_owned);
                for stage in stages {
                    if self.control.is_aborted() {
                        break;
                    }
                    // A stage may itself be a pipeline; boxing the
                    // recursive call keeps the future finite.
                    let executed =
                        Box::pin(self.run_step(stage, output.as_deref()))
                            .await?;
                    calls.extend(executed.calls);
                    output = executed.output;
                }
                Ok(Execution { calls, output })
            }
            PlannedStep::Mcts {
                id,
                concurrency,
                branches,
                depths,
                scorer,
            } => {
                let kind: BuiltinScorerKind = (*scorer).into();
                let builtin = match kind {
                    BuiltinScorerKind::Heuristic => {
                        let heuristic = self
                            .heuristic_scorer
                            .as_ref()
                            .ok_or_else(|| WorkflowError::MctsScorer {
                                step_id: id.clone(),
                                scorer: *scorer,
                            })?;
                        BuiltinScorer::Heuristic(Arc::clone(heuristic))
                    }
                    BuiltinScorerKind::ShortestText => {
                        BuiltinScorer::ShortestText
                    }

                    BuiltinScorerKind::LongestText => {
                        BuiltinScorer::LongestText
                    }
                };
                self.run_mcts(
                    id,
                    *concurrency,
                    branches,
                    depths,
                    &builtin,
                    input,
                )
                .await
            }
        }
    }

    /// Run a step's calls concurrently, under the step's own bound when
    /// it set one.
    ///
    /// Step-level admission sits outside run-level admission: a call
    /// holds its step slot while it waits for a run slot, and nothing
    /// takes them the other way round (steps run in sequence, so one
    /// step's bound cannot block another's). A fan-out and a `best_of`
    /// step ask for exactly the same work here and differ only in what
    /// they do with the results.
    async fn run_items(
        &self,
        items: &[usize],
        concurrency: Option<usize>,
        input: Option<&str>,
    ) -> Result<Vec<CallRun>, WorkflowError> {
        let bound = concurrency.map(Semaphore::new);
        let runs = join_all(items.iter().map(|index| {
            let call = self.call(*index);
            let bound = bound.as_ref();
            async move {
                let _step_permit = match bound {
                    Some(semaphore) => match semaphore.acquire().await {
                        Ok(permit) => Some(permit),
                        // Only the run's semaphore is drained; a drained
                        // step bound admits nothing.
                        Err(_drained) => return Ok(CallRun::aborted(call)),
                    },
                    None => None,
                };
                self.run_call(call, input).await
            }
        }))
        .await;
        let mut calls = Vec::with_capacity(runs.len());
        for run in runs {
            calls.push(run?);
        }
        Ok(calls)
    }

    async fn run_call(
        &self,
        call: &PlannedCall,
        input: Option<&str>,
    ) -> Result<CallRun, WorkflowError> {
        if let Some(entry) = self.replay.get(&call.position) {
            // Re-recorded so this run's journal stands on its own: a
            // resume of a resume must not have to walk back a chain of
            // earlier files to find the prefix. A selection's decision
            // is deliberately left behind: it belongs to the run that
            // made it, and this run writes the one it makes.
            let mut recorded = entry.clone();
            recorded.winner = false;
            self.record(recorded);
            return Ok(CallRun::replayed(call, entry));
        }

        let request = self.request(call, chained_prompt(&call.prompt, input));
        loop {
            match self.attempt(call, &request).await? {
                Attempt::Aborted => return Ok(CallRun::aborted(call)),
                // Paused after it was admitted: the permit went back and
                // the call parks at the gate like everything else.
                Attempt::Paused => continue,
                Attempt::Skipped => {
                    self.record_failure(call, SKIPPED_MESSAGE);
                    return Ok(CallRun::skipped(call));
                }
                Attempt::Settled { text, error, gate } => {
                    // A request is consumed by the settle it covers,
                    // whether or not it was needed.
                    let retried = self.control.take_retry(&call.step_id);
                    if error.is_some() && retried {
                        continue;
                    }
                    return Ok(match error {
                        Some(message) => {
                            self.record_failure(call, &message);
                            CallRun::failed(call, message, gate)
                        }
                        None => {
                            let text = text.unwrap_or_default();
                            self.record_success(call, &text);
                            CallRun::ok(call, text, gate)
                        }
                    });
                }
            }
        }
    }

    /// One attempt at a call: gate, admit, spawn, gate command.
    async fn attempt(
        &self,
        call: &PlannedCall,
        request: &AgentRequest,
    ) -> Result<Attempt, WorkflowError> {
        // The pause gate is taken before the slot: a paused run must not
        // sit on concurrency it is not using while its running agents
        // drain.
        self.control.wait_until_open().await;
        if self.control.is_aborted() {
            return Ok(Attempt::Aborted);
        }

        let _permit = match self.semaphore.acquire().await {
            Ok(permit) => permit,
            // Aborted while queued: the call never starts.
            Err(_drained) => return Ok(Attempt::Aborted),
        };
        if self.control.is_aborted() {
            return Ok(Attempt::Aborted);
        }
        if self.control.is_skipped(&call.step_id) {
            return Ok(Attempt::Skipped);
        }
        if self.control.is_paused() {
            return Ok(Attempt::Paused);
        }

        let outcome = match self.host.spawn_agent(request).await {
            Ok(outcome) => outcome,
            Err(source) => {
                // The host could not make the call at all. Recorded so a
                // resume re-runs it, then reported: carrying on would
                // only pile up failures the document cannot see.
                self.record_failure(call, &source.to_string());
                return Err(WorkflowError::Host {
                    step_id: call.step_id.clone(),
                    source: Box::new(source),
                });
            }
        };
        let AgentOutcome { text, error } = outcome;
        if let Some(message) = error {
            return Ok(Attempt::Settled {
                text: None,
                error: Some(message),
                // The gate never ran for this call, so it has no
                // verdict to report.
                gate: GateVerdict::Unobserved,
            });
        }

        // The gate runs after the agent, and only for one that
        // succeeded: a failing gate fails the call.
        let gate = match call.gate.as_ref() {
            Some(gate) => gate,
            None => {
                return Ok(Attempt::Settled {
                    text,
                    error: None,
                    gate: GateVerdict::Absent,
                });
            }
        };
        let gate_outcome = match self.host.run_gate(gate, self.cwd).await {
            Ok(outcome) => outcome,
            Err(source) => {
                self.record_failure(call, &source.to_string());
                return Err(WorkflowError::Gate {
                    gate: gate.command.clone(),
                    step_id: call.step_id.clone(),
                    source: Box::new(source),
                });
            }
        };
        if !gate_outcome.passed {
            return Ok(Attempt::Settled {
                text: None,
                error: Some(gate_failure(gate, &gate_outcome.output)),
                gate: GateVerdict::Failed,
            });
        }
        Ok(Attempt::Settled {
            text,
            error: None,
            gate: GateVerdict::Passed,
        })
    }

    /// Decide a `best_of` step: which candidate won.
    ///
    /// The decision comes from the journal when the whole step did.
    /// Nothing about a step's candidates but its decision depends on
    /// anything outside the journal, so a step answered entirely from it
    /// carries a decision that was already made: it is read back, never
    /// asked again — a selector may be a model call, and paying for it
    /// twice is exactly what the journal exists to avoid. A decision the
    /// journal does not carry — a run killed between its last candidate
    /// settling and its decision, or a journal written before selections
    /// could be judged — falls back to the built-in rule: no decision
    /// was made, so nothing that was recorded is re-decided.
    ///
    /// As soon as one candidate ran here the step's inputs are no longer
    /// the ones the journal decided, so the host is asked; and it is
    /// asked only then, and never for a run control has already aborted.
    async fn select(
        &self,
        step_id: &str,
        items: &[usize],
        input: Option<&str>,
        calls: Vec<CallRun>,
    ) -> Result<Execution, WorkflowError> {
        let ran = calls.iter().any(|call| call.status.crossed_host());
        let chosen = if ran && !self.control.is_aborted() {
            self.host_choice(step_id, items, input, &calls).await?
        } else {
            self.recorded_winner(&calls)
                .or_else(|| first_success(&calls))
        };
        if let Some(call) = chosen.and_then(|index| calls.get(index)) {
            self.record_winner(call);
        }
        Ok(selection(calls, chosen))
    }

    /// Ask the host to pick, and check what it picked.
    ///
    /// A selector that names a candidate which cannot win has answered
    /// wrongly, and that is reported rather than papered over: falling
    /// back to the built-in rule would turn a broken judge into a
    /// plausible winner. The run stops, and because every candidate has
    /// already settled and been journaled, that costs nothing which has
    /// not been paid.
    async fn host_choice(
        &self,
        step_id: &str,
        items: &[usize],
        input: Option<&str>,
        calls: &[CallRun],
    ) -> Result<Option<usize>, WorkflowError> {
        let request = self.selection_request(step_id, items, input, calls);
        let selected = match self.host.select_candidate(&request).await {
            Ok(selected) => selected,
            Err(source) => {
                return Err(WorkflowError::Host {
                    step_id: step_id.to_owned(),
                    source: Box::new(source),
                });
            }
        };
        match selected {
            None => Ok(first_success(calls)),
            Some(index) => match calls.get(index) {
                Some(call) if call.succeeded() => Ok(Some(index)),
                Some(call) => Err(WorkflowError::Selection {
                    step_id: step_id.to_owned(),
                    selected: index,
                    reason: rejection_reason(call),
                }),
                None => Err(WorkflowError::Selection {
                    step_id: step_id.to_owned(),
                    selected: index,
                    reason: format!(
                        "the step has only {} candidates",
                        calls.len()
                    ),
                }),
            },
        }
    }

    /// The request a selector is given: everything a judge needs, and
    /// nothing about how the run is executed.
    fn selection_request(
        &self,
        step_id: &str,
        items: &[usize],
        input: Option<&str>,
        calls: &[CallRun],
    ) -> SelectionRequest {
        // A `best_of` step always plans at least one candidate — that is
        // what makes it a selection at all — so the step's shared agent
        // and gate are the first candidate's.
        let step =
            self.call(*items.first().expect("a best_of step has candidates"));
        SelectionRequest {
            run_id: self.run_id.to_owned(),
            step_id: step_id.to_owned(),
            agent: step.agent.clone(),
            gate: step.gate.clone(),
            candidates: items
                .iter()
                .zip(calls)
                .map(|(position, call)| SelectionCandidate {
                    prompt: chained_prompt(&self.call(*position).prompt, input),
                    outcome: candidate_outcome(call),
                    gate: call.gate,
                })
                .collect(),
        }
    }

    /// The candidate a decision already on record points at.
    ///
    /// The entry must still match the plan and the call must have
    /// succeeded here: a decision is reused only for the candidates it
    /// was made about, and only when the call it names is still a win.
    fn recorded_winner(&self, calls: &[CallRun]) -> Option<usize> {
        calls.iter().position(|call| {
            call.succeeded()
                && self.replay.get(&call.position).is_some_and(|entry| {
                    entry.winner
                        && entry.key == self.call(call.position).identity
                })
        })
    }

    /// Write the step's decision down: a later run replays the choice
    /// instead of asking a selector to make it again.
    ///
    /// The line is written for the winner's position — the decision is a
    /// fact about that candidate — and lands after the call's own
    /// record, because the last line for a position is the one a replay
    /// reads.
    fn record_winner(&self, call: &CallRun) {
        let entry = match self.replay.get(&call.position) {
            // A replayed winner keeps its recorded time and text; a live
            // one is entered as it settled.
            Some(entry) => entry.clone(),
            None => JournalEntry::success(
                call.position,
                self.call(call.position).identity.clone(),
                call.text.clone().unwrap_or_default(),
            ),
        };
        self.record(entry.selecting());
    }

    fn call(&self, index: usize) -> &PlannedCall {
        &self.plan.calls()[index]
    }

    fn request(&self, call: &PlannedCall, prompt: String) -> AgentRequest {
        AgentRequest {
            run_id: self.run_id.to_owned(),
            step_id: call.step_id.clone(),
            position: call.position,
            depth: call.depth,
            agent: call.agent.clone(),
            prompt,
            gate: call.gate.clone(),
            isolation: call.isolation,
            cwd: self.cwd.map(Path::to_path_buf),
        }
    }

    fn record_success(&self, call: &PlannedCall, text: &str) {
        self.record(JournalEntry::success(
            call.position,
            call.identity.clone(),
            text,
        ));
    }

    fn record_failure(&self, call: &PlannedCall, message: &str) {
        self.record(JournalEntry::failure(
            call.position,
            call.identity.clone(),
            message,
        ));
    }

    fn record(&self, entry: JournalEntry) {
        let Some(path) = self.journal_path else {
            return;
        };
        if let Err(error) = append_journal(path, &entry) {
            // An append that fails costs a future resume, never the run.
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "workflow journal append failed",
            );
        }
    }
}
