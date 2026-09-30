//! Declarative workflow: fan-out, a gated call, and journal replay.
//!
//! Seam shown: [`WorkflowRuntime`] executes a serialisable
//! [`WorkflowSpec`] while *all* real work goes through one injected
//! [`WorkflowHost`]. The host here is a fake (`EchoHost`) — no model,
//! no network, no API key — so the example shows exactly what the
//! runtime asks the host for and what it does with the answers:
//! statuses per call, gate verdicts, and a second run that replays
//! from the journal instead of spawning again.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-workflow --example workflow_fanout
//! ```
//!
//! Look at: the printed `[position] step status` lines — the fan-out
//! produces one call per item, the gated call is only `Succeeded`
//! because the host passed its gate, and the replay run reports the
//! same texts without touching the host again.

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

/// A host with no model behind it: it echoes the prompt, fails any
/// gate whose command starts with `fail`, and counts how many agent
/// calls actually crossed it.
#[derive(Default)]
struct EchoHost {
    spawned: Mutex<Vec<String>>,
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
        if gate.command.starts_with("fail") {
            return Ok(GateOutcome::failed("tests failed: 2 assertions"));
        }
        Ok(GateOutcome::passed())
    }
}

fn print_run(title: &str, run: &WorkflowRun) {
    println!("== {title}");
    println!(
        "run {}: spawned={} replayed={}",
        run.run_id, run.spawned, run.replayed
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // A workflow is data: fan out three files to one agent, then a
    // gated single call whose prompt is the wire-serialisable shape.
    let spec = WorkflowSpec::from_json(
        r#"{
          "id": "audit",
          "steps": [
            {
              "kind": "fan_out",
              "id": "scan",
              "agent": "scanner",
              "items": ["src/a.rs", "src/b.rs", "src/c.rs"],
              "concurrency": 2
            },
            {
              "kind": "agent",
              "id": "verify",
              "agent": "reviewer",
              "prompt": "check the scan results",
              "gate": { "command": "cargo test -p synthia-core" }
            }
          ]
        }"#,
    )
    .expect("the inline spec is valid JSON");

    let journal = std::env::temp_dir().join(format!(
        "synthia-workflow-example-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&journal);

    let host = Arc::new(EchoHost::default());
    let runtime = WorkflowRuntime::new(Arc::clone(&host))
        .with_max_concurrency(2)
        .with_journal(&journal);

    let first = runtime.run(&spec).await.expect("first run");
    print_run("first run (host spawns)", &first);
    println!("host calls: {}", host.spawned());

    // The same runtime again: every call comes back from the journal,
    // so the model is not asked twice and nothing is re-charged.
    let replay = runtime.run(&spec).await.expect("replay run");
    print_run("second run (journal replay)", &replay);
    println!("host calls after replay: {}", host.spawned());

    // A gate that fails is data, not an error: the call settles
    // `Failed`, the run still finishes, and the failure text is
    // something a caller can report.
    let failing = WorkflowSpec::from_json(
        r#"{
          "id": "audit",
          "steps": [{
            "kind": "agent",
            "id": "verify",
            "agent": "reviewer",
            "prompt": "check the scan results",
            "gate": { "command": "fail -p synthia-core" }
          }]
        }"#,
    )
    .expect("spec");
    let failed = runtime.run(&failing).await.expect("run with failing gate");
    print_run("gated call, failing gate", &failed);

    let _ = std::fs::remove_file(&journal);
    // 3 fan-out items + the gated call; the replay spawned nothing,
    // and the changed gate command is a different call, so it re-ran.
    assert_eq!(host.spawned(), 5, "see the printed host-call counts");
    println!("WORKFLOW-FANOUT: OK");
}
