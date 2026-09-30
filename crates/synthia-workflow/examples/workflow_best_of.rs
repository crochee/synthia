//! Declarative workflow: a `best_of` selection, a gate, a host that
//! judges, and journal replay.
//!
//! Seam shown: [`WorkflowRuntime`] executes a serialisable
//! [`WorkflowSpec`] while *all* real work goes through one injected
//! [`WorkflowHost`]. The hosts here are fakes (`EchoHost`, `JudgeHost`)
//! — no model, no network, no API key — so the example shows exactly
//! what a selection asks the host for and what it does with the
//! answers: every candidate spawns, every candidate's gate runs, and
//! the winner is the candidate the host's
//! [`select_candidate`] chose.
//!
//! Two selections, one document. `EchoHost` declines to judge, so the
//! built-in rule stands: the first candidate that passed its gate.
//! `JudgeHost` judges and keeps the longest answer — which is *not* the
//! first that passed, so the step reports a different winner and hands
//! a different text on. Both runs write their decision to the journal,
//! so replaying either reproduces its winner without asking the host to
//! decide again.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-workflow --example workflow_best_of
//! ```
//!
//! Look at: the printed `[position] step status` lines — the losing
//! candidates are `Superseded` (they ran, they are not the step's
//! failure), the winner is `Succeeded`, and each replay run reports the
//! same winner without touching its host again.
//!
//! [`select_candidate`]: synthia_workflow::WorkflowHost::select_candidate

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use synthia_workflow::{
    AgentOutcome,
    AgentRequest,
    CandidateOutcome,
    GateOutcome,
    GateRef,
    SelectionRequest,
    WorkflowError,
    WorkflowHost,
    WorkflowRun,
    WorkflowRuntime,
    WorkflowSpec,
};

/// A host with no model behind it: it answers every candidate, grades it
/// by its prompt (`one` is the candidate that fails), and counts how many
/// agent calls actually crossed it.
#[derive(Default)]
struct EchoHost {
    spawned: Mutex<Vec<String>>,
    graded: Mutex<usize>,
}

impl EchoHost {
    fn spawned(&self) -> usize {
        self.spawned.lock().len()
    }
}

#[async_trait]
impl WorkflowHost for EchoHost {
    async fn spawn_agent(
        &self,
        request: &AgentRequest,
    ) -> Result<AgentOutcome, WorkflowError> {
        self.spawned
            .lock()
            .push(format!("{}#{}", request.agent, request.position));
        Ok(AgentOutcome::ok(format!(
            "{} handled: {}",
            request.agent, request.prompt
        )))
    }

    async fn run_gate(
        &self,
        gate: &GateRef,
        _cwd: Option<&Path>,
    ) -> Result<GateOutcome, WorkflowError> {
        // A command starting with `fail` fails every candidate that
        // reaches it. Any other gate stands in for "did this
        // candidate's work pass?" and fails the first candidate to
        // reach it, so the selection has to look at the next one.
        if gate.command.starts_with("fail") {
            return Ok(GateOutcome::failed("tests failed: 2 assertions"));
        }
        let mut graded = self.graded.lock();
        *graded += 1;
        if *graded == 1 {
            return Ok(GateOutcome::failed("tests failed: 2 assertions"));
        }
        Ok(GateOutcome::passed())
    }
}

/// A host that judges instead of taking the first candidate that passed:
/// among the candidates that settled as successes it keeps the longest
/// answer. Everything else — spawning, gates — is the echo host, so only
/// the decision differs between the two halves of this example.
#[derive(Default)]
struct JudgeHost {
    inner: EchoHost,
    judged: Mutex<usize>,
}

impl JudgeHost {
    /// How many times a selection was put to this judge. A replay must
    /// not raise it.
    fn judged(&self) -> usize {
        *self.judged.lock()
    }
}

#[async_trait]
impl WorkflowHost for JudgeHost {
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
        *self.judged.lock() += 1;
        println!(
            "judge: judging {} candidates of `{}` (gate: {})",
            request.candidates.len(),
            request.step_id,
            request
                .gate
                .as_ref()
                .map(|gate| gate.command.as_str())
                .unwrap_or("none"),
        );
        for (index, candidate) in request.candidates.iter().enumerate() {
            println!("  [{index}] {} ({:?})", candidate.prompt, candidate.gate);
        }
        // Only a candidate that settled as a success can win; the
        // runtime rejects any other answer rather than falling back.
        let longest = request
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| match &candidate.outcome {
                CandidateOutcome::Succeeded { text } => {
                    Some((index, text.len()))
                }
                _ => None,
            })
            .max_by_key(|(_, length)| *length)
            .map(|(index, _)| index);
        println!("judge: picking candidate {longest:?} (longest answer)");
        Ok(longest)
    }
}

fn print_run(title: &str, run: &WorkflowRun) {
    println!("== {title}");
    println!(
        "run {}: spawned={} replayed={}",
        run.run_id, run.spawned, run.replayed
    );
    println!(
        "winner: {}",
        run.winner_of("pick")
            .and_then(|call| call.text.as_deref())
            .unwrap_or("(nobody)")
    );
    for call in &run.calls {
        println!(
            "  [{:>2}] {:<8} {:<10} {}",
            call.position,
            call.step_id,
            format!("{:?}", call.status),
            call.text
                .as_deref()
                .map(|text| text.replace('\n', " / "))
                .unwrap_or_else(|| call
                    .error
                    .clone()
                    .unwrap_or_else(|| "(no output)".to_string()))
        );
    }
}

/// A journal path of this process's own, so a stale file cannot answer a
/// call this run should make.
fn journal_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "synthia-workflow-best-of-{}-{tag}.jsonl",
        std::process::id()
    ))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // A workflow is data: three candidates under one agent, each gated,
    // then a single call whose prompt is the wire-serialisable shape.
    let spec = WorkflowSpec::from_json(
        r#"{
          "id": "pick",
          "steps": [
            {
              "kind": "best_of",
              "id": "pick",
              "agent": "coder",
              "items": ["attempt one", "attempt two", "attempt three"],
              "gate": { "command": "cargo test -p synthia-core" },
              "concurrency": 2
            },
            {
              "kind": "agent",
              "id": "ship",
              "agent": "reviewer",
              "prompt": "review the winning attempt"
            }
          ]
        }"#,
    )
    .expect("the inline spec is valid JSON");

    let journal = journal_path("builtin");
    let _ = std::fs::remove_file(&journal);

    let host = Arc::new(EchoHost::default());
    let runtime = WorkflowRuntime::new(Arc::clone(&host))
        .with_max_concurrency(2)
        .with_journal(&journal);

    let first = runtime.run(&spec).await.expect("first run");
    print_run(
        "first run (host declines to judge: the gate decides)",
        &first,
    );
    println!("host calls: {}", host.spawned());

    // The same runtime again: the candidates, the loss the gate already
    // decided and the step after it all come back from the journal, so
    // the model is not asked twice and nothing is re-charged.
    let replay = runtime.run(&spec).await.expect("replay run");
    print_run("second run (journal replay)", &replay);
    println!("host calls after replay: {}", host.spawned());

    // The same document, judged instead: a host that returns an index
    // changes the winner — and therefore the text the step reports —
    // without the document changing at all.
    let judged_journal = journal_path("judged");
    let _ = std::fs::remove_file(&judged_journal);
    let judge = Arc::new(JudgeHost::default());
    let judged_runtime = WorkflowRuntime::new(Arc::clone(&judge))
        .with_max_concurrency(2)
        .with_journal(&judged_journal);

    let judged = judged_runtime.run(&spec).await.expect("judged run");
    print_run("third run (host judges: longest answer wins)", &judged);
    println!("judge calls: {}", judge.judged());

    // The decision is journaled with the candidates, so replaying the
    // judged run reproduces the choice instead of paying the judge — an
    // LLM call in a real host — a second time.
    let judged_replay = judged_runtime.run(&spec).await.expect("judged replay");
    print_run(
        "fourth run (judged selection, journal replay)",
        &judged_replay,
    );
    println!("judge calls after replay: {}", judge.judged());

    // A selection no candidate can win is still a run that finishes:
    // every statement of what failed is there for a caller to report,
    // and the step reports no output.
    let impossible = WorkflowSpec::from_json(
        r#"{
          "id": "pick",
          "steps": [{
            "kind": "best_of",
            "id": "pick",
            "agent": "coder",
            "items": ["attempt one", "attempt two"],
            "gate": { "command": "fail -p synthia-core" }
          }]
        }"#,
    )
    .expect("spec");
    let failed = runtime.run(&impossible).await.expect("run nobody can win");
    assert!(!failed.succeeded(), "nobody won this one");
    print_run("selection nobody wins", &failed);

    // The two rules have to disagree here, or this example proves
    // nothing about who decides.
    assert_ne!(
        judged.winner_of("pick").unwrap().text,
        first.winner_of("pick").unwrap().text,
        "the judge must pick a different candidate than the built-in rule",
    );
    assert_eq!(
        judged_replay.winner_of("pick").unwrap().position,
        judged.winner_of("pick").unwrap().position,
    );
    assert_eq!(judge.judged(), 1, "a replay must not ask the judge again");
    let _ = std::fs::remove_file(&journal);
    let _ = std::fs::remove_file(&judged_journal);
    // Three candidates + the review call; the replay spawned nothing,
    // the third spec is a different document whose candidates all fail
    // their gate, so it spawned twice more.
    assert_eq!(host.spawned(), 6, "see the printed host-call counts");
    // The judged run's host did the same three candidates and the review
    // call once, and its replay spawned nothing.
    assert_eq!(judge.inner.spawned(), 4, "see the printed host-call counts");
    println!("WORKFLOW-BEST-OF: OK");
}
