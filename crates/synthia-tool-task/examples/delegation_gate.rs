//! # Delegation verification gate
//!
//! Seam: `synthia_tool_task::gate` — a `GateSpec` shell command
//! runs after a delegated child finishes. `run_gate` produces one
//! verdict; `apply_gate_to_output` turns it into the parent's tool
//! result: the child's text on pass, a `failed gate: ...` error
//! carrying the gate's exit status and output otherwise.
//!
//! Run: cargo run -p synthia-tool-task --example delegation_gate

use synthia_provider::ContentPart;
use synthia_tool::ToolOutput;
use synthia_tool_task::gate::{
    GateSpec,
    StdCommandRunner,
    apply_gate_to_output,
    no_cancel_token,
    run_gate,
    verdict_summary,
};

const CHILD_TEXT: &str = "child agent: edited 2 files, ran 1 command";

fn text(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(ContentPart::text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn main() {
    let cwd = std::env::current_dir().expect("current working directory");
    let runner = StdCommandRunner::new();

    let passing = GateSpec::new("sh")
        .arg("-c")
        .arg("echo gate: all checks passed");
    let verdict = run_gate(&passing, &cwd, &runner, no_cancel_token())
        .expect("passing gate runs");
    let pass_output =
        apply_gate_to_output(Some(CHILD_TEXT), &verdict, passing.label());
    println!("pass verdict: {}", verdict_summary(&verdict));
    println!("pass result: {}", text(&pass_output));

    let failing = GateSpec::new("sh")
        .arg("-c")
        .arg("echo '2 tests failed' >&2; exit 3");
    let verdict = run_gate(&failing, &cwd, &runner, no_cancel_token())
        .expect("failing gate runs");
    let fail_output =
        apply_gate_to_output(Some(CHILD_TEXT), &verdict, failing.label());
    println!("fail verdict: {}", verdict_summary(&verdict));
    println!("fail result:\n{}", text(&fail_output));

    assert!(
        !pass_output.is_error.unwrap_or(false),
        "a passing gate returns the child text unchanged",
    );
    assert_eq!(fail_output.is_error, Some(true));
    assert!(
        text(&fail_output).contains("failed gate"),
        "a failing gate is prefixed for the parent model",
    );
    println!("DELEGATION-GATE: OK");
}
