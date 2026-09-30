//! Declarative workflow: an MCTS branching search.
//!
//! Seam shown: [`WorkflowRuntime`] executes a serialisable
//! [`WorkflowSpec`] while *all* real work goes through one injected
//! [`WorkflowHost`]. The host here is a fake (`MctsHost`) — no model,
//! no network — so the example shows exactly what an MCTS step asks
//! of the runtime: branches and depths planned in pre-order, every
//! depth's prompt chained off the same branch's previous depth, the
//! scorer picking the winner, the journal answering a replay.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-workflow --example workflow_mcts
//! ```
//!
//! Look at: the printed `[position] step status` lines — every
//! branch runs `max_depth + 1` calls, the depth-`max_depth` text is
//! the one the scorer sees, and a second run replays from the
//! journal without crossing the host again.

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use synthia_workflow::{
    AgentOutcome,
    AgentRequest,
    GateOutcome,
    GateRef,
    WorkflowError,
    WorkflowHost,
    WorkflowRun,
    WorkflowRuntime,
    WorkflowSpec,
};

/// A host whose only job is to echo the prompt — and to record every
/// depth's prompt so the example can show that depth 2 carries depth
/// 1's text and depth 1 carries depth 0's. Nothing here waits on
/// anything, so the depth rows run as the runtime orders them.
#[derive(Default)]
struct MctsHost {
    spawned: Mutex<Vec<String>>,
}

impl MctsHost {
    fn spawned(&self) -> usize {
        self.spawned.lock().len()
    }

    fn prompts(&self) -> Vec<String> {
        self.spawned.lock().clone()
    }
}

#[async_trait]
impl WorkflowHost for MctsHost {
    async fn spawn_agent(
        &self,
        request: &AgentRequest,
    ) -> Result<AgentOutcome, WorkflowError> {
        self.spawned.lock().push(request.prompt.clone());
        Ok(AgentOutcome::ok(format!(
            "answer for branch {}: {}",
            request.position, request.prompt
        )))
    }

    async fn run_gate(
        &self,
        _gate: &GateRef,
        _cwd: Option<&Path>,
    ) -> Result<GateOutcome, WorkflowError> {
        Ok(GateOutcome::passed())
    }
}

fn print_run(title: &str, run: &WorkflowRun) {
    println!("== {title}");
    println!(
        "run {}: spawned={} replayed={}",
        run.run_id, run.spawned, run.replayed
    );
    if let Some(text) = run.output.as_deref() {
        println!("step output: {}", text.replace('\n', " / "));
    } else {
        println!("step output: (nobody)");
    }
    for call in &run.calls {
        let branch_score = call
            .branch_score
            .as_ref()
            .map(|score| {
                format!(
                    "score={}{}",
                    score.score.unwrap_or(0.0),
                    if score.winner { " WINNER" } else { "" }
                )
            })
            .unwrap_or_default();
        println!(
            "  [{:>2}] {:<8} {:<12} {}{}",
            call.position,
            call.step_id,
            format!("{:?}", call.status),
            call.text
                .as_deref()
                .map(|text| text.replace('\n', " / "))
                .unwrap_or_else(|| {
                    call.error
                        .clone()
                        .unwrap_or_else(|| "(no output)".to_string())
                }),
            branch_score,
        );
    }
}

/// A journal path of this process's own, so a stale file cannot
/// answer a call this run should make.
fn journal_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "synthia-workflow-mcts-{}-{tag}.jsonl",
        std::process::id()
    ))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Three branches, two additional depths (so depth 0, 1, 2 run per
    // branch — 9 calls total), the longest-text scorer. A pipeline
    // stage after the MCTS step chains its winner's text on.
    let spec = WorkflowSpec::from_json(
        r#"{
          "id": "explore",
          "steps": [
            {
              "kind": "mcts",
              "id": "explore",
              "agent": "coder",
              "prompt": "solve the puzzle",
              "branches": 3,
              "max_depth": 2,
              "scorer": { "kind": "longest_text" }
            },
            {
              "kind": "agent",
              "id": "ship",
              "agent": "reviewer",
              "prompt": "summarize the winning reasoning"
            }
          ]
        }"#,
    )
    .expect("the inline spec is valid JSON");

    let journal = journal_path("builtin");
    let _ = std::fs::remove_file(&journal);

    let host = Arc::new(MctsHost::default());
    let runtime = WorkflowRuntime::new(Arc::clone(&host))
        .with_max_concurrency(4)
        .with_journal(&journal);

    let first = runtime.run(&spec).await.expect("first run");
    println!("host spawns: {}", host.spawned());
    let chained = host.prompts();
    let deeper = chained
        .iter()
        .filter(|p| p.contains("--- output from the previous stage ---"))
        .count();
    println!(
        "prompts with a chain marker: {} (depth 1 + 2 of every branch)",
        deeper
    );
    assert_eq!(
        host.spawned(),
        10,
        "9 MCTS calls + 1 reviewer after the step"
    );
    assert_eq!(deeper, 6, "only depth 0 calls are unchained");

    // A second run replays from the journal without crossing the
    // host: every call's status is `Replayed`, the winner is the
    // same one the first run picked.
    let replay = runtime.run(&spec).await.expect("replay");
    print_run("second run (journal replay)", &replay);
    assert_eq!(replay.replayed, 10, "every call came back from the journal");
    assert_eq!(
        first.winner_of("explore").unwrap().position,
        replay.winner_of("explore").unwrap().position,
        "a replay must reproduce the same winner"
    );
    assert_eq!(first.output, replay.output);

    let _ = std::fs::remove_file(&journal);
    println!("WORKFLOW-MCTS: OK");
}
