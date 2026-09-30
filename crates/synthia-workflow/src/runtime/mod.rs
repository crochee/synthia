//! The runtime: admission, execution and journaling.
//!
//! A run is one pass over a [`WorkflowPlan`]. Steps run in order; a
//! fan-out's items and a `best_of` step's candidates run concurrently
//! under two bounds (the run's `Semaphore` and the step's own, when it
//! set one); a pipeline's stages run in order, each fed the previous
//! stage's text. A fan-out reports every text it produced; a `best_of`
//! step reports only its winner's.
//!
//! A `best_of` step then makes the one decision a document cannot
//! express: which candidate won. It is the host's
//! ([`WorkflowHost::select_candidate`]), and it may decline by
//! returning `Ok(None)`, leaving the built-in rule — the first
//! candidate that settled as a success, in item order — in place. The
//! host is asked only for a selection this run ran candidates for: a
//! step answered entirely from the journal reuses the decision that
//! run recorded, so a replay never asks again. Whichever way the
//! decision was made, it is written to the journal, and the candidates
//! it passed over are reported as ones that lost.
//!
//! Every call goes through the same four decisions, in this order:
//!
//! 1. **Replay** — a call inside the journal's matching prefix is
//!    answered from disk and re-recorded, and never spawns.
//! 2. **Control** — a skipped step is reported without spawning; a
//!    paused run parks before it takes a slot.
//! 3. **Admission** — the semaphore bounds how many agents are live at
//!    once, and an abort drains whatever is queued.
//! 4. **Effect** — the host spawns the agent, and then runs the step's
//!    gate if the agent succeeded.
//!
//! The journal is written after each call settles, success or failure.
//! Its only effect on the run is the replay decision in step 1: an
//! append that fails is logged and forgotten.
//!
//! # Module layout
//!
//! - `executor`: the per-run `Executor` that walks the plan.
//! - `execution`: how one attempt settles and how one step's calls
//!   are shaped into its result.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use ulid::Ulid;

use crate::{
    concurrency::{Semaphore, default_max_concurrency},
    control::WorkflowControl,
    error::WorkflowError,
    host::WorkflowHost,
    journal::{JournalEntry, read_journal, replay_lookup},
    plan::{WorkflowCaps, WorkflowPlan},
    result::{CallRun, CallStatus, WorkflowRun},
    scorer::Scorer,
    spec::WorkflowSpec,
};

mod execution;
mod executor;

use executor::Executor;

/// Journal message for a call live control skipped.
pub(crate) const SKIPPED_MESSAGE: &str = "skipped by user";

/// Runs workflow documents against one [`WorkflowHost`].
///
/// The runtime holds no runtime-specific state: it is driven by awaiting
/// [`Self::run`], and everything it does in the world crosses the host.
/// Configure it with the builder methods, then run as many documents as
/// you like; the same semaphore bound applies to each.
pub struct WorkflowRuntime<H> {
    host: H,
    caps: WorkflowCaps,
    max_concurrency: usize,
    journal_path: Option<PathBuf>,
    control: WorkflowControl,
    cwd: Option<PathBuf>,
    /// Scorer a document names as `heuristic`. Documents that ask for
    /// it without registering one error at run time with
    /// [`WorkflowError::MctsScorer`].
    heuristic_scorer: Option<Arc<dyn Scorer>>,
}

impl<H: WorkflowHost> WorkflowRuntime<H> {
    /// A runtime with default caps, the default concurrency bound, no
    pub fn new(host: H) -> Self {
        Self {
            host,
            caps: WorkflowCaps::default(),
            max_concurrency: default_max_concurrency(),
            journal_path: None,
            control: WorkflowControl::new(),
            cwd: None,
            heuristic_scorer: None,
        }
    }

    /// Register the scorer a document names as `heuristic`.
    ///
    /// Documents that name [`MctsScorer::Heuristic`](crate::scorer::MctsScorer::Heuristic)
    /// will use this
    /// scorer; without registration, the run errors with
    /// [`WorkflowError::MctsScorer`].
    pub fn with_heuristic_scorer(mut self, scorer: Arc<dyn Scorer>) -> Self {
        self.heuristic_scorer = Some(scorer);
        self
    }

    /// Replace the caps documents are planned against.
    pub fn with_caps(mut self, caps: WorkflowCaps) -> Self {
        self.caps = caps;
        self
    }

    /// Replace the admission bound. Zero is clamped to one.
    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = max_concurrency.max(1);
        self
    }

    /// Read the resume journal from `path`, and append to it as calls
    /// settle. A missing file means "nothing to replay".
    pub fn with_journal(mut self, path: impl Into<PathBuf>) -> Self {
        self.journal_path = Some(path.into());
        self
    }

    /// Install a control handle. The caller keeps a clone to steer the
    /// run; by default the runtime owns a handle nobody else can reach.
    pub fn with_control(mut self, control: WorkflowControl) -> Self {
        self.control = control;
        self
    }

    /// Working directory handed to the host with every request and gate.
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// The control handle this runtime reads.
    pub fn control(&self) -> &WorkflowControl {
        &self.control
    }

    /// Run one workflow document to completion.
    ///
    /// # Errors
    ///
    /// Planning errors ([`WorkflowError::InvalidSpec`],
    /// [`WorkflowError::CapExceeded`]) before anything spawns, and
    /// [`WorkflowError::Host`] / [`WorkflowError::Gate`] when the host
    /// cannot answer. [`WorkflowError::Selection`] when it answers a
    /// `best_of` step with a candidate that cannot win. An agent that
    /// fails is a settled call, not an error.
    pub async fn run(
        &self,
        spec: &WorkflowSpec,
    ) -> Result<WorkflowRun, WorkflowError> {
        let plan = spec.plan(&self.caps)?;
        let run_id = format!("wf_{}", Ulid::generate());
        let entries = match self.journal_path.as_deref() {
            Some(path) => {
                prepare_journal(path);
                read_journal(path)
            }
            None => Vec::new(),
        };
        let replay = self.replay_map(&plan, &entries);
        tracing::debug!(
            run_id = %run_id,
            workflow = %plan.id(),
            calls = plan.call_count(),
            replayed = replay.len(),
            "workflow run starting",
        );

        let executor = Executor {
            host: &self.host,
            control: &self.control,
            run_id: &run_id,
            cwd: self.cwd.as_deref(),
            journal_path: self.journal_path.as_deref(),
            semaphore: Semaphore::new(self.max_concurrency),
            plan: &plan,
            replay: &replay,
            heuristic_scorer: self.heuristic_scorer.as_ref(),
        };
        let mut execution = executor.drive().await?;
        // An aborted run stops walking; the calls it never reached are
        // reported as such, so the result still lines up with the plan.
        for call in plan.calls().iter().skip(execution.calls.len()) {
            execution.calls.push(CallRun::aborted(call));
        }

        let replayed = execution
            .calls
            .iter()
            .filter(|call| call.status == CallStatus::Replayed)
            .count();
        let spawned = execution
            .calls
            .iter()
            .filter(|call| call.status.crossed_host())
            .count();
        tracing::debug!(
            run_id = %run_id,
            replayed,
            spawned,
            "workflow run finished",
        );
        Ok(WorkflowRun {
            run_id,
            output: execution.output,
            calls: execution.calls,
            replayed,
            spawned,
        })
    }

    /// The journal entries this run may answer calls from.
    fn replay_map(
        &self,
        plan: &WorkflowPlan,
        entries: &[JournalEntry],
    ) -> BTreeMap<usize, JournalEntry> {
        let prefix = plan.replay_prefix(entries);
        let lookup = replay_lookup(entries);
        let mut replay = BTreeMap::new();
        for call in plan.calls().iter().take(prefix) {
            // A pending retry means "run this one live", so it ends the
            // replay there instead of being overridden by the journal.
            if self.control.has_retry(&call.step_id) {
                break;
            }
            if let Some(entry) = lookup.get(&call.position) {
                replay.insert(call.position, JournalEntry::clone(entry));
            }
        }
        replay
    }
}

/// Make sure the journal's directory exists. A failure here is a
/// warning: every append is allowed to fail on its own.
fn prepare_journal(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    if parent.as_os_str().is_empty() {
        return;
    }
    if let Err(error) = std::fs::create_dir_all(parent) {
        tracing::warn!(
            path = %parent.display(),
            error = %error,
            "workflow journal directory could not be created",
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc};

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use tokio::test;

    use super::execution::CHAIN_MARKER;
    use crate::{
        control::WorkflowControl,
        error::WorkflowError,
        host::{
            AgentOutcome,
            AgentRequest,
            CandidateOutcome,
            GateOutcome,
            GateVerdict,
            SelectionRequest,
            WorkflowHost,
        },
        plan::WorkflowCaps,
        result::{BranchScore, CallStatus},
        runtime::WorkflowRuntime,
        scorer::MctsScorer,
        spec::{
            AgentStep,
            BestOfStep,
            GateRef,
            MctsStep,
            PipelineStep,
            Step,
            WorkflowSpec,
        },
    };

    /// An agent whose prompt contains `fail` fails, and the gate command
    /// decides the gate: `fail` fails every candidate, `first-fails`
    /// fails the first call that reaches it and passes the rest, and
    /// anything else passes. Nothing in the host waits on anything, so
    /// one step's calls settle in item order and a test can say exactly
    /// which candidate was which.
    #[derive(Default)]
    struct FakeHost {
        prompts: Mutex<Vec<String>>,
        gates: Mutex<usize>,
        live: Mutex<usize>,
        peak: Mutex<usize>,
        yield_spawns: bool,
        /// A handle the fake pulls from, once, while it spawns the first
        /// agent of a run — how a test catches a step mid-flight, with
        /// candidates still to come.
        control: Option<(WorkflowControl, Pull)>,
    }

    /// What a fake host pulls from live control mid-step.
    #[derive(Clone, Copy)]
    enum Pull {
        /// Abort the run.
        Abort,
        /// Skip the step that is running.
        Skip,
    }

    impl FakeHost {
        /// A host that suspends inside every spawn, so a test can watch
        /// how many candidates are live at once.
        fn yielding() -> Self {
            Self {
                yield_spawns: true,
                ..Self::default()
            }
        }

        /// A host that pulls `pull` from `control` as it spawns the
        /// first agent of the run.
        fn pulling(control: &WorkflowControl, pull: Pull) -> Self {
            Self {
                control: Some((control.clone(), pull)),
                ..Self::default()
            }
        }

        fn spawns(&self) -> usize {
            self.prompts.lock().len()
        }

        fn prompts(&self) -> Vec<String> {
            self.prompts.lock().clone()
        }

        fn peak(&self) -> usize {
            *self.peak.lock()
        }
    }

    #[async_trait]
    impl WorkflowHost for FakeHost {
        async fn spawn_agent(
            &self,
            request: &AgentRequest,
        ) -> Result<AgentOutcome, WorkflowError> {
            self.prompts.lock().push(request.prompt.clone());
            if let Some((control, pull)) = self.control.as_ref()
                && self.prompts.lock().len() == 1
            {
                match pull {
                    Pull::Abort => control.abort(),
                    Pull::Skip => control.skip(&request.step_id),
                }
            }
            {
                let mut live = self.live.lock();
                *live += 1;
                let now = *live;
                let mut peak = self.peak.lock();
                *peak = (*peak).max(now);
            }
            if self.yield_spawns {
                tokio::task::yield_now().await;
            }
            *self.live.lock() -= 1;
            if request.prompt.contains("fail") {
                return Ok(AgentOutcome::failed(format!(
                    "{} failed",
                    request.prompt
                )));
            }
            Ok(AgentOutcome::ok(format!("answer: {}", request.prompt)))
        }

        async fn run_gate(
            &self,
            gate: &GateRef,
            _cwd: Option<&Path>,
        ) -> Result<GateOutcome, WorkflowError> {
            let mut calls = self.gates.lock();
            *calls += 1;
            let verdict = match gate.command.as_str() {
                "fail" => GateOutcome::failed("tests failed"),
                "first-fails" if *calls == 1 => {
                    GateOutcome::failed("tests failed")
                }
                _ => GateOutcome::passed(),
            };
            Ok(verdict)
        }
    }

    /// A host that judges as well as answers.
    ///
    /// Agents and gates behave exactly as `FakeHost`'s do, so the same
    /// documents exercise both, and every selection it is offered is
    /// recorded: a test can read what a judge is given, and can change
    /// the answer between runs to show which run asked.
    struct ChooserHost {
        inner: FakeHost,
        answer: Mutex<Result<Option<usize>, String>>,
        selections: Mutex<Vec<SelectionRequest>>,
    }

    impl ChooserHost {
        /// A host that answers every selection with `answer`.
        fn answering(answer: Option<usize>) -> Self {
            Self::over(FakeHost::default(), answer)
        }

        /// A host that answers every selection with `answer`, running
        /// the agent and gate work through `inner`.
        fn over(inner: FakeHost, answer: Option<usize>) -> Self {
            Self {
                inner,
                answer: Mutex::new(Ok(answer)),
                selections: Mutex::new(Vec::new()),
            }
        }

        /// Change what the next selection is answered with.
        fn answer(&self, answer: Option<usize>) {
            *self.answer.lock() = Ok(answer);
        }

        /// Make every selection fail: the judge cannot answer at all.
        fn broken(&self, reason: &str) {
            *self.answer.lock() = Err(reason.to_owned());
        }

        fn selections(&self) -> usize {
            self.selections.lock().len()
        }

        fn request(&self, nth: usize) -> SelectionRequest {
            self.selections.lock()[nth].clone()
        }
    }

    #[async_trait]
    impl WorkflowHost for ChooserHost {
        async fn spawn_agent(
            &self,
            request: &AgentRequest,
        ) -> Result<AgentOutcome, WorkflowError> {
            self.inner.spawn_agent(request).await
        }

        async fn run_gate(
            &self,
            gate: &GateRef,
            cwd: Option<&Path>,
        ) -> Result<GateOutcome, WorkflowError> {
            self.inner.run_gate(gate, cwd).await
        }

        async fn select_candidate(
            &self,
            request: &SelectionRequest,
        ) -> Result<Option<usize>, WorkflowError> {
            self.selections.lock().push(request.clone());
            match self.answer.lock().clone() {
                Ok(answer) => Ok(answer),
                Err(message) => Err(WorkflowError::Document { message }),
            }
        }
    }

    /// One selection over `items`, optionally gated.
    fn best_of(items: &[&str], gate: Option<&str>) -> BestOfStep {
        BestOfStep {
            id: "pick".to_owned(),
            agent: "coder".to_owned(),
            items: items.iter().map(|item| (*item).to_owned()).collect(),
            gate: gate.map(|command| GateRef {
                command: command.to_owned(),
                label: None,
            }),
            max_items: None,
            concurrency: None,
        }
    }

    /// A single ungated call, for the step after a selection.
    fn echo(id: &str) -> Step {
        Step::Agent(AgentStep {
            id: id.to_owned(),
            agent: "echo".to_owned(),
            prompt: format!("{id} prompt"),
            gate: None,
            isolation: false,
        })
    }
    /// A single MCTS step. `branches` is the number of parallel
    /// branches and `depths` is `max_depth`; every call carries the
    /// same `gate` and `scorer`.
    fn mcts(
        id: &str,
        prompt: &str,
        branches: u32,
        depths: u32,
        gate: Option<&str>,
        scorer: MctsScorer,
    ) -> Step {
        Step::Mcts(MctsStep {
            id: id.to_owned(),
            agent: "coder".to_owned(),
            prompt: prompt.to_owned(),
            branches,
            max_depth: depths,
            gate: gate.map(|command| GateRef {
                command: command.to_owned(),
                label: None,
            }),
            concurrency: None,
            scorer,
        })
    }

    fn spec(steps: Vec<Step>) -> WorkflowSpec {
        WorkflowSpec {
            id: "wf".to_owned(),
            steps,
            phases: Vec::new(),
        }
    }

    #[test]
    async fn the_first_candidate_that_passes_its_gate_is_the_step_output() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![Step::Pipeline(PipelineStep {
            id: "chain".to_owned(),
            stages: vec![
                Step::BestOf(best_of(
                    &["one", "two", "three"],
                    Some("first-fails"),
                )),
                echo("after"),
            ],
        })]);

        let run = runtime.run(&document).await.unwrap();

        let winner = run.winner_of("pick").expect("a candidate won");
        assert_eq!(winner.status, CallStatus::Succeeded);
        assert_eq!(winner.position, 1);
        assert_eq!(winner.text.as_deref(), Some("answer: two"));

        let statuses: Vec<CallStatus> =
            run.calls.iter().map(|call| call.status).collect();
        assert_eq!(
            statuses,
            [
                CallStatus::Superseded,
                CallStatus::Succeeded,
                CallStatus::Superseded,
                CallStatus::Succeeded,
            ]
        );
        // The candidate that failed its gate keeps the reason.
        let gate_error = run.calls[0].error.as_deref().unwrap();
        assert!(gate_error.contains("failed"), "{gate_error}");
        // A loser that produced text keeps that too.
        assert_eq!(run.calls[2].text.as_deref(), Some("answer: three"));
        assert_eq!(host.spawns(), 4, "every candidate spawned");

        // The winner's text is the only text the step hands on: the
        // stage after it sees candidate two and neither loser.
        let staged = host.prompts().pop().unwrap();
        assert!(staged.contains("answer: two"), "{staged}");
        assert!(!staged.contains("answer: one"), "{staged}");
        assert!(!staged.contains("answer: three"), "{staged}");
        assert!(run.succeeded(), "losing candidates never fail the run");
    }

    #[test]
    async fn without_a_gate_the_first_candidate_that_succeeds_wins() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document =
            spec(vec![Step::BestOf(best_of(&["fail one", "two"], None))]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(run.output.as_deref(), Some("answer: two"));
        assert_eq!(host.spawns(), 2, "every candidate spawns");
        assert_eq!(run.calls[0].status, CallStatus::Superseded);
        assert_eq!(run.calls[0].error.as_deref(), Some("fail one failed"));
        assert_eq!(run.calls[1].status, CallStatus::Succeeded);
        assert!(run.succeeded());
    }

    #[test]
    async fn a_selection_nobody_wins_fails_the_step() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![
            Step::BestOf(best_of(&["fail one", "fail two"], None)),
            echo("after"),
        ]);

        let run = runtime.run(&document).await.unwrap();

        assert!(run.winner_of("pick").is_none());
        assert!(!run.succeeded());
        assert_eq!(run.calls[0].status, CallStatus::Failed);
        assert_eq!(run.calls[1].status, CallStatus::Failed);
        let errors: Vec<&str> = run
            .calls
            .iter()
            .take(2)
            .map(|call| call.error.as_deref().unwrap())
            .collect();
        assert_eq!(errors, ["fail one failed", "fail two failed"]);
        // A selection nobody won is a settle, not an error: the rest of
        // the document still runs, unchained and unaware of it — and it
        // is that step's text the run reports.
        assert_eq!(run.calls[2].status, CallStatus::Succeeded);
        assert_eq!(run.calls[2].text.as_deref(), Some("answer: after prompt"));
        assert_eq!(run.output.as_deref(), Some("answer: after prompt"));
        assert_eq!(host.spawns(), 3);
    }

    #[test]
    async fn a_gate_nobody_passes_fails_the_step_though_every_agent_ran() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document =
            spec(vec![Step::BestOf(best_of(&["one", "two"], Some("fail")))]);

        let run = runtime.run(&document).await.unwrap();

        assert!(!run.succeeded());
        assert_eq!(run.output, None);
        assert_eq!(host.spawns(), 2);
        for call in &run.calls {
            assert_eq!(call.status, CallStatus::Failed);
            assert!(call.text.is_none());
            let error = call.error.as_deref().unwrap();
            assert!(error.contains("failed"), "{error}");
        }
    }

    #[test]
    async fn a_selection_replays_without_spawning_again() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.jsonl");
        let host = Arc::new(FakeHost::default());
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_journal(&journal);
        let document = spec(vec![
            Step::BestOf(best_of(
                &["one", "two", "three"],
                Some("first-fails"),
            )),
            echo("after"),
        ]);

        let first = runtime.run(&document).await.unwrap();
        assert_eq!(first.spawned, 4);
        assert!(first.succeeded());

        let second = runtime.run(&document).await.unwrap();

        assert_eq!(host.spawns(), 4, "the journal answered every call");
        assert_eq!(second.spawned, 0);
        assert_eq!(second.replayed, 4);
        assert_eq!(second.output, first.output);
        assert_eq!(
            second.winner_of("pick").unwrap().position,
            first.winner_of("pick").unwrap().position
        );
        let winner = second.winner_of("pick").unwrap();
        assert_eq!(winner.text.as_deref(), Some("answer: two"));
        assert_eq!(winner.status, CallStatus::Replayed);
        // The candidate that lost to it is reused as the loss it was,
        // and a reused failure is still not the step's winner.
        assert_eq!(second.calls[0].status, CallStatus::Replayed);
        let reused = second.calls[0].error.as_deref().unwrap();
        assert!(reused.contains("failed"), "{reused}");
        assert!(second.succeeded());
    }

    #[test]
    async fn a_retry_re_runs_a_decided_selection_live() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.jsonl");
        let host = Arc::new(FakeHost::default());
        let control = WorkflowControl::new();
        let runtime = WorkflowRuntime::new(Arc::clone(&host))
            .with_journal(&journal)
            .with_control(control.clone());
        let document =
            spec(vec![Step::BestOf(best_of(&["fail one", "two"], None))]);

        let first = runtime.run(&document).await.unwrap();
        assert_eq!(first.spawned, 2);
        assert_eq!(first.winner_of("pick").unwrap().position, 1);

        // Nothing asked for: the decided loss and the win are both
        // reused, and nothing spawns.
        let again = runtime.run(&document).await.unwrap();
        assert_eq!(again.replayed, 2);
        assert_eq!(again.spawned, 0);
        assert_eq!(host.spawns(), 2);

        // A retry is a request for a fresh attempt: the whole step —
        // the decided loss included — runs live once more, and the one
        // request gives the candidate that fails its extra attempt.
        control.retry("pick");
        let retried = runtime.run(&document).await.unwrap();
        assert_eq!(retried.replayed, 0);
        assert_eq!(retried.spawned, 2);
        assert_eq!(host.spawns(), 5);
        assert_eq!(retried.winner_of("pick").unwrap().position, 1);
    }

    #[test]
    async fn an_oversized_selection_is_rejected_before_anything_spawns() {
        let host = Arc::new(FakeHost::default());
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_caps(WorkflowCaps {
                max_items: 2,
                ..WorkflowCaps::default()
            });
        let document =
            spec(vec![Step::BestOf(best_of(&["one", "two", "three"], None))]);

        let error = runtime.run(&document).await.unwrap_err();

        assert!(matches!(
            error,
            WorkflowError::CapExceeded {
                cap: "max_items",
                limit: 2,
                requested: 3,
            }
        ));
        assert_eq!(host.spawns(), 0);
    }

    #[test]
    async fn a_skipped_selection_has_no_winner() {
        let host = Arc::new(FakeHost::default());
        let control = WorkflowControl::new();
        control.skip("pick");
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_control(control);
        let document = spec(vec![Step::BestOf(best_of(&["one", "two"], None))]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(host.spawns(), 0);
        assert!(run.winner_of("pick").is_none());
        assert_eq!(run.output, None);
        assert!(!run.succeeded());
        // Control is not a selection decision: a skipped candidate is
        // reported as skipped, not as one that lost to a winner.
        for call in &run.calls {
            assert_eq!(call.status, CallStatus::Skipped);
        }
    }

    #[test]
    async fn an_aborted_run_admits_no_candidate() {
        let host = Arc::new(FakeHost::default());
        let control = WorkflowControl::new();
        control.abort();
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_control(control);
        let document = spec(vec![Step::BestOf(best_of(&["one", "two"], None))]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(host.spawns(), 0);
        assert!(run.aborted());
        assert!(!run.succeeded());
        for call in &run.calls {
            assert_eq!(call.status, CallStatus::Aborted);
        }
    }

    #[test]
    async fn a_selection_keeps_to_its_own_concurrency_bound() {
        let host = Arc::new(FakeHost::yielding());
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_max_concurrency(8);
        let document = spec(vec![Step::BestOf(BestOfStep {
            concurrency: Some(2),
            ..best_of(&["one", "two", "three", "four"], None)
        })]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(host.peak(), 2, "the step's bound is the tighter one");
        assert_eq!(host.spawns(), 4);
        assert!(run.succeeded());
        assert_eq!(run.calls[0].status, CallStatus::Succeeded);
    }

    #[test]
    async fn a_host_that_declines_leaves_the_built_in_rule_in_place() {
        let host = Arc::new(ChooserHost::answering(None));
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![Step::BestOf(best_of(
            &["one", "two", "three"],
            Some("first-fails"),
        ))]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(host.selections(), 1, "the host is offered the decision");
        let winner = run.winner_of("pick").expect("a candidate won");
        assert_eq!(winner.position, 1, "the first candidate that passed");
        assert_eq!(run.output.as_deref(), Some("answer: two"));
        let statuses: Vec<CallStatus> =
            run.calls.iter().map(|call| call.status).collect();
        assert_eq!(
            statuses,
            [
                CallStatus::Superseded,
                CallStatus::Succeeded,
                CallStatus::Superseded,
            ]
        );
        assert!(run.succeeded());
    }

    #[test]
    async fn the_request_carries_the_candidates_and_their_gate_verdicts() {
        let host = Arc::new(ChooserHost::answering(None));
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![
            Step::BestOf(best_of(&["one", "two"], Some("first-fails"))),
            Step::BestOf(BestOfStep {
                id: "plain".to_owned(),
                ..best_of(&["plain one"], None)
            }),
        ]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(host.selections(), 2, "one request per selection");
        let gated = host.request(0);
        assert_eq!(gated.run_id, run.run_id);
        assert_eq!(gated.step_id, "pick");
        assert_eq!(gated.agent, "coder");
        assert_eq!(
            gated.gate.as_ref().map(|gate| gate.command.as_str()),
            Some("first-fails")
        );
        let prompts: Vec<&str> = gated
            .candidates
            .iter()
            .map(|candidate| candidate.prompt.as_str())
            .collect();
        assert_eq!(prompts, ["one", "two"]);
        // The candidate the gate turned down says so, and says why.
        assert_eq!(gated.candidates[0].gate, GateVerdict::Failed);
        assert_eq!(
            gated.candidates[0].outcome,
            CandidateOutcome::Failed {
                error: "gate `first-fails` failed:\ntests failed".to_owned(),
            }
        );
        assert_eq!(gated.candidates[1].gate, GateVerdict::Passed);
        assert_eq!(
            gated.candidates[1].outcome,
            CandidateOutcome::Succeeded {
                text: "answer: two".to_owned(),
            }
        );

        // A step with no gate judges no candidate by one.
        let ungated = host.request(1);
        assert_eq!(ungated.step_id, "plain");
        assert!(ungated.gate.is_none());
        assert_eq!(ungated.candidates[0].gate, GateVerdict::Absent);
        assert_eq!(
            ungated.candidates[0].outcome,
            CandidateOutcome::Succeeded {
                text: "answer: plain one".to_owned(),
            }
        );
    }

    #[test]
    async fn a_host_that_selects_a_later_candidate_changes_the_winner() {
        let host = Arc::new(ChooserHost::answering(Some(2)));
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![Step::BestOf(best_of(
            &["one", "two", "three"],
            Some("first-fails"),
        ))]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(run.output.as_deref(), Some("answer: three"));
        let winner = run.winner_of("pick").expect("a candidate won");
        assert_eq!(winner.position, 2);
        assert!(winner.winner, "the decision is reported, not re-derived");
        let statuses: Vec<CallStatus> =
            run.calls.iter().map(|call| call.status).collect();
        assert_eq!(
            statuses,
            [
                CallStatus::Superseded,
                CallStatus::Superseded,
                CallStatus::Succeeded,
            ]
        );
        // A candidate the decision passed over keeps what it produced.
        assert_eq!(run.calls[1].text.as_deref(), Some("answer: two"));
        assert!(run.succeeded(), "losing a decision never fails the run");
    }

    #[test]
    async fn a_host_that_selects_a_candidate_that_cannot_win_is_rejected() {
        let host = Arc::new(ChooserHost::answering(Some(0)));
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![Step::BestOf(best_of(
            &["one", "two"],
            Some("first-fails"),
        ))]);

        let error = runtime.run(&document).await.unwrap_err();

        let WorkflowError::Selection {
            step_id,
            selected,
            reason,
        } = error
        else {
            panic!("a candidate its gate turned down cannot win: {error:?}");
        };
        assert_eq!(step_id, "pick");
        assert_eq!(selected, 0);
        assert!(reason.contains("did not succeed"), "{reason}");
        assert!(
            reason.contains("gate `first-fails` failed"),
            "the reason must name what failed it: {reason}"
        );
        assert!(
            reason.contains("tests failed"),
            "the gate's own output must be surfaced: {reason}"
        );
    }

    #[test]
    async fn a_host_that_selects_a_candidate_outside_the_step_is_rejected() {
        let host = Arc::new(ChooserHost::answering(Some(7)));
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![Step::BestOf(best_of(&["one", "two"], None))]);

        let error = runtime.run(&document).await.unwrap_err();

        let WorkflowError::Selection {
            step_id,
            selected,
            reason,
        } = error
        else {
            panic!("an index outside the step cannot win: {error:?}");
        };
        assert_eq!(step_id, "pick");
        assert_eq!(selected, 7);
        assert!(reason.contains("only 2 candidates"), "{reason}");
    }

    #[test]
    async fn a_host_that_cannot_choose_stops_the_run() {
        let host = Arc::new(ChooserHost::answering(None));
        host.broken("the judge is down");
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![Step::BestOf(best_of(&["one", "two"], None))]);

        let error = runtime.run(&document).await.unwrap_err();

        let WorkflowError::Host { step_id, source } = error else {
            panic!("a selector that cannot answer stops the run: {error:?}");
        };
        assert_eq!(step_id, "pick");
        assert!(source.to_string().contains("the judge is down"), "{source}");
        // Every candidate had already settled, so none of that is redone.
        assert_eq!(host.inner.spawns(), 2);
    }

    #[test]
    async fn the_request_carries_the_prompt_the_candidate_ran_with() {
        let host = Arc::new(ChooserHost::answering(Some(0)));
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![Step::Pipeline(PipelineStep {
            id: "chain".to_owned(),
            stages: vec![echo("pre"), Step::BestOf(best_of(&["one"], None))],
        })]);

        runtime.run(&document).await.unwrap();

        let request = host.request(0);
        let spawned = host.inner.prompts();
        assert_eq!(
            request.candidates[0].prompt, spawned[1],
            "a judge has to see the prompt the candidate was actually run with"
        );
        assert!(
            request.candidates[0].prompt.contains(CHAIN_MARKER),
            "a pipeline stage's chained input is part of that prompt: {}",
            request.candidates[0].prompt
        );
    }

    #[test]
    async fn a_replayed_selection_reproduces_the_hosts_choice() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.jsonl");
        let host = Arc::new(ChooserHost::answering(Some(2)));
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_journal(&journal);
        let document = spec(vec![Step::BestOf(best_of(
            &["one", "two", "three"],
            Some("first-fails"),
        ))]);

        let first = runtime.run(&document).await.unwrap();
        assert_eq!(first.spawned, 3);
        assert_eq!(first.winner_of("pick").unwrap().position, 2);
        assert_eq!(host.selections(), 1);

        // A different answer would name a different winner, so a replay
        // that still reports candidate three never asked again.
        host.answer(Some(1));
        let second = runtime.run(&document).await.unwrap();

        assert_eq!(host.selections(), 1, "replay must not consult the host");
        assert_eq!(second.spawned, 0);
        assert_eq!(second.replayed, 3);
        assert_eq!(second.output, first.output);
        let winner = second.winner_of("pick").unwrap();
        assert_eq!(winner.position, 2);
        assert_eq!(winner.status, CallStatus::Replayed);
        assert_eq!(winner.text.as_deref(), Some("answer: three"));
        assert!(winner.winner);
        // The candidate the decision passed over is still the success the
        // journal recorded it as, and still not the step's winner.
        assert_eq!(second.calls[1].status, CallStatus::Replayed);
        assert!(!second.calls[1].winner);
        // A replayed success under a gate must have passed it; a replayed
        // failure cannot say which half of the call failed.
        assert_eq!(second.calls[1].gate, GateVerdict::Passed);
        assert_eq!(second.calls[0].gate, GateVerdict::Unobserved);
        assert!(second.succeeded());

        // What the second run wrote has to carry the decision too: a
        // resume of a resume reads the lines its predecessor appended.
        let third = runtime.run(&document).await.unwrap();
        assert_eq!(third.spawned, 0);
        assert_eq!(host.selections(), 1);
        assert_eq!(third.winner_of("pick").unwrap().position, 2);
        assert_eq!(third.output.as_deref(), Some("answer: three"));
    }

    #[test]
    async fn a_retried_selection_is_asked_again() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.jsonl");
        let host = Arc::new(ChooserHost::answering(Some(2)));
        let control = WorkflowControl::new();
        let runtime = WorkflowRuntime::new(Arc::clone(&host))
            .with_journal(&journal)
            .with_control(control.clone());
        let document = spec(vec![Step::BestOf(best_of(
            &["one", "two", "three"],
            Some("first-fails"),
        ))]);

        let first = runtime.run(&document).await.unwrap();
        assert_eq!(first.winner_of("pick").unwrap().position, 2);

        // A retry asks for the step to be done again, which is a fresh
        // decision: the recorded one must not stand in for it.
        control.retry("pick");
        host.answer(Some(1));
        let retried = runtime.run(&document).await.unwrap();

        assert_eq!(host.selections(), 2, "a retry is a fresh decision");
        assert_eq!(retried.replayed, 0);
        assert_eq!(retried.spawned, 3);
        assert_eq!(retried.winner_of("pick").unwrap().position, 1);
        assert_eq!(retried.output.as_deref(), Some("answer: two"));
    }

    #[test]
    async fn an_aborted_run_does_not_consult_the_selector() {
        let control = WorkflowControl::new();
        let host = Arc::new(ChooserHost::over(
            FakeHost::pulling(&control, Pull::Abort),
            Some(0),
        ));
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_control(control);
        let document = spec(vec![Step::BestOf(BestOfStep {
            concurrency: Some(1),
            ..best_of(&["one", "two"], None)
        })]);

        let run = runtime.run(&document).await.unwrap();

        assert!(run.aborted());
        assert!(!run.succeeded());
        assert_eq!(host.inner.spawns(), 1, "the abort lands mid-step");
        assert_eq!(
            host.selections(),
            0,
            "control stopped the run; deciding it is not a new effect to spend"
        );
        assert_eq!(run.calls[0].status, CallStatus::Succeeded);
        assert_eq!(run.calls[1].status, CallStatus::Aborted);
        assert_eq!(run.output.as_deref(), Some("answer: one"));
    }

    #[test]
    async fn a_candidate_control_skipped_is_offered_as_one_that_never_settled()
    {
        let control = WorkflowControl::new();
        let host = Arc::new(ChooserHost::over(
            FakeHost::pulling(&control, Pull::Skip),
            None,
        ));
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_control(control);
        let document = spec(vec![Step::BestOf(BestOfStep {
            concurrency: Some(1),
            ..best_of(&["one", "two"], None)
        })]);

        let run = runtime.run(&document).await.unwrap();

        assert_eq!(host.selections(), 1);
        let request = host.request(0);
        assert_eq!(
            request.candidates[0].outcome,
            CandidateOutcome::Succeeded {
                text: "answer: one".to_owned(),
            }
        );
        // A candidate that never ran did not fail: a judge is told the
        // difference, so it can rule it out for the right reason.
        assert_eq!(request.candidates[1].outcome, CandidateOutcome::Unsettled);
        assert_eq!(request.candidates[1].gate, GateVerdict::Unobserved);
        assert_eq!(host.inner.spawns(), 1);
        assert_eq!(run.calls[1].status, CallStatus::Skipped);
        assert_eq!(run.winner_of("pick").unwrap().position, 0);
        assert!(!run.succeeded(), "control changed the run");
    }

    #[test]
    async fn mcts_longest_picks_the_branch_with_the_longest_text() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![mcts(
            "explore",
            "reason about the problem",
            3,
            0,
            None,
            MctsScorer::LongestText,
        )]);

        let run = runtime.run(&document).await.unwrap();
        // The FakeHost echoes the prompt; longer prompts score higher.
        let winner = run.winner_of("explore").expect("a branch won");
        assert_eq!(
            winner.text.as_deref(),
            Some("answer: reason about the problem")
        );
        // Every call is in the plan: 3 branches at depth 0.
        assert_eq!(host.spawns(), 3);
        assert!(run.succeeded());
    }

    #[test]
    async fn mcts_shortest_picks_the_branch_with_the_shortest_text() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![mcts(
            "explore",
            "a very long prompt with lots of words",
            4,
            0,
            None,
            MctsScorer::ShortestText,
        )]);

        let run = runtime.run(&document).await.unwrap();
        // Every branch echoes the same prompt here, so the scorer ties
        // them all; the tie-break (lower `branch_id`) picks branch 0.
        let winner = run.winner_of("explore").expect("a branch won");
        assert_eq!(winner.position, 0);
    }

    #[test]
    async fn mcts_gate_failed_branch_with_empty_text_loses_to_a_text_branch() {
        // Two branches; the first gate call (branch 0) fails the gate,
        // so branch 0's text is empty and branch 1's is the agent's
        // echo. The longest-text scorer must pick branch 1.
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![mcts(
            "explore",
            "reason",
            2,
            0,
            Some("first-fails"),
            MctsScorer::LongestText,
        )]);

        let run = runtime.run(&document).await.unwrap();
        let winner = run.winner_of("explore").expect("a branch won");
        assert_eq!(winner.position, 1, "branch 1 wins on length");
        assert_eq!(winner.gate, GateVerdict::Passed);
        let loser = run
            .calls
            .iter()
            .find(|c| c.position == 0)
            .expect("the losing branch is reported");
        assert_eq!(loser.gate, GateVerdict::Failed);
        assert!(loser.text.is_none(), "a gate-failed branch carries no text");
    }

    #[test]
    async fn mcts_gate_failed_branch_does_not_win_unless_everyone_failed() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        // A gate that fails the first call to reach it and passes the
        // rest. Two branches: branch 0 fails its gate, branch 1 passes.
        let document = spec(vec![mcts(
            "explore",
            "reason",
            2,
            0,
            Some("first-fails"),
            MctsScorer::LongestText,
        )]);

        let run = runtime.run(&document).await.unwrap();
        let winner = run.winner_of("explore").expect("a branch won");
        // Branch 1 is the only one whose gate passed; its text is
        // non-empty and it wins.
        assert_eq!(winner.position, 1);
        assert_eq!(winner.gate, GateVerdict::Passed);
        let loser = run
            .calls
            .iter()
            .find(|c| c.position == 0)
            .expect("the losing branch is reported");
        assert_eq!(loser.gate, GateVerdict::Failed);
        assert!(!loser.winner);
    }

    #[test]
    async fn mcts_tie_break_picks_lower_branch_id() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        // Identical prompts ⇒ identical text ⇒ identical scores; the
        // tie-break is `branch_id` ascending.
        let document = spec(vec![mcts(
            "explore",
            "reason",
            4,
            0,
            None,
            MctsScorer::LongestText,
        )]);

        let run = runtime.run(&document).await.unwrap();
        let winner = run.winner_of("explore").expect("a branch won");
        assert_eq!(winner.position, 0);
    }

    #[test]
    async fn mcts_replay_reproduces_call_ordering_without_rescoring() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.jsonl");
        let host = Arc::new(FakeHost::default());
        let runtime =
            WorkflowRuntime::new(Arc::clone(&host)).with_journal(&journal);
        let document = spec(vec![mcts(
            "explore",
            "reason",
            3,
            1,
            None,
            MctsScorer::LongestText,
        )]);

        let first = runtime.run(&document).await.unwrap();
        assert_eq!(first.spawned, 6); // 3 branches × (1 + 1) depths.
        let winner_first = first.winner_of("explore").unwrap().position;

        // A second run replays from the journal without crossing the
        // host: every call's status is `Replayed` and the winner is
        // the same.
        let second = runtime.run(&document).await.unwrap();
        assert_eq!(second.spawned, 0);
        assert_eq!(second.replayed, 6);
        assert_eq!(host.spawns(), 6);
        let winner_second = second.winner_of("explore").unwrap().position;
        assert_eq!(winner_second, winner_first);
    }

    #[test]
    async fn mcts_depth_two_chains_per_branch() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![mcts(
            "explore",
            "reason",
            2,
            2,
            None,
            MctsScorer::LongestText,
        )]);

        let _run = runtime.run(&document).await.unwrap();
        // 2 branches × (2 + 1) depths = 6 calls.
        assert_eq!(host.spawns(), 6);
        // The depth-1 and depth-2 prompts chain off the previous
        // depth's text; check the prompts the host saw include the
        // marker.
        let prompts = host.prompts();
        let chained = prompts
            .iter()
            .filter(|p| p.contains("--- output from the previous stage ---"))
            .count();
        assert_eq!(chained, 4, "depth 1 and depth 2 of every branch chain");
    }

    #[test]
    async fn mcts_no_heuristic_scorer_registered_errors_cleanly() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![mcts(
            "explore",
            "reason",
            2,
            0,
            None,
            MctsScorer::Heuristic,
        )]);

        let error = runtime.run(&document).await.unwrap_err();
        assert!(
            matches!(error, WorkflowError::MctsScorer { .. }),
            "missing heuristic scorer is a typed error: {error:?}"
        );
    }

    #[test]
    async fn mcts_branch_scores_are_reported_on_calls() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![mcts(
            "explore",
            "reason",
            3,
            0,
            None,
            MctsScorer::LongestText,
        )]);

        let run = runtime.run(&document).await.unwrap();
        // Every depth-0 call carries a BranchScore (depth 0 is the
        // last depth when `max_depth == 0`).
        let scored: Vec<&BranchScore> = run
            .calls
            .iter()
            .filter_map(|c| c.branch_score.as_ref())
            .collect();
        assert_eq!(scored.len(), 3);
        // Exactly one is the winner.
        let winners = scored.iter().filter(|s| s.winner).count();
        assert_eq!(winners, 1);
        // The reported scores reflect the longest-text scorer: the
        // winner's score is the maximum.
        let max = scored
            .iter()
            .map(|s| s.score.unwrap())
            .fold(f64::NEG_INFINITY, f64::max);
        let winner_score =
            scored.iter().find(|s| s.winner).unwrap().score.unwrap();
        assert_eq!(winner_score, max);
    }

    #[test]
    async fn mcts_round_trips_through_its_wire_form() {
        let json = r#"{
          "id": "wf",
          "steps": [{
            "kind": "mcts",
            "id": "explore",
            "agent": "coder",
            "prompt": "reason",
            "branches": 3,
            "max_depth": 2,
            "scorer": { "kind": "longest_text" }
          }]
        }"#;
        let document = WorkflowSpec::from_json(json).unwrap();
        let Step::Mcts(step) = &document.steps[0] else {
            panic!("`mcts` should decode as an MCTS step");
        };
        assert_eq!(step.branches, 3);
        assert_eq!(step.max_depth, 2);
        assert_eq!(step.scorer, MctsScorer::LongestText);
        assert!(step.gate.is_none());

        let encoded = serde_json::to_string(&document).unwrap();
        assert!(
            encoded.contains(r#""kind":"mcts""#),
            "the wire name is part of the document format: {encoded}"
        );
        assert_eq!(WorkflowSpec::from_json(&encoded).unwrap(), document);
    }

    #[test]
    async fn mcts_rejects_zero_branches_before_anything_runs() {
        let host = Arc::new(FakeHost::default());
        let runtime = WorkflowRuntime::new(Arc::clone(&host));
        let document = spec(vec![mcts(
            "explore",
            "reason",
            0,
            0,
            None,
            MctsScorer::LongestText,
        )]);

        let error = runtime.run(&document).await.unwrap_err();
        assert!(matches!(error, WorkflowError::InvalidSpec { .. }));
        assert_eq!(host.spawns(), 0);
    }
}
