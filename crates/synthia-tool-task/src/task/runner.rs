use std::sync::Arc;

use futures::StreamExt;
use synthia_core::CancelToken;
use synthia_harness::{
    Agent,
    AgentEvent,
    AgentInput,
    AgentMeta,
    SessionEndReason,
    SystemEvent,
};
use synthia_provider::{ContentPart, TextContent};

use super::spec::TaskSpec;
use crate::{
    gate::{CommandRunner, StdCommandRunner, apply_gate_to_output, run_gate},
    worktree::{self, WorktreeCleanupResult, WorktreeInfo},
};

/// Run one sub-agent and translate its event stream into a tool
/// result.
///
/// Every child event is forwarded through `on_event` wrapped in
/// [`AgentEvent::Agent`] with an [`AgentMeta`] tying it to the
/// parent (`parent_session_id`, `child_depth` =
/// `input_depth + 1`). The child's accumulated `Model` text
/// becomes the tool result; a cancelled or failed child becomes
/// an error result so the parent model sees a usable signal.
///
/// # Gate + isolation lifecycle
///
/// When `spec.gate` is `Some`, the gate runs **after** the child
/// completes (and **after** the worktree is set up, if isolation
/// is set). The gate's cwd is the worktree path when isolation
/// is on, otherwise the parent base cwd. A passing gate lets
/// the child's text through; a failing / timed-out / killed
/// gate replaces the tool result with a structured error.
///
/// When `spec.isolation` is `Some`, the child runs against a
/// fresh detached worktree and the worktree is cleaned up
/// **before** the gate runs, so the gate observes whatever the
/// child committed (or, for dirty trees, whatever the child left
/// on disk at cleanup time). Cleanup reports branch + path +
/// `base_sha` when changes are present so the caller can
/// surface them.
pub(crate) async fn run_subagent(
    agent: Arc<dyn Agent>,
    spec: TaskSpec,
    parent_session_id: &str,
    input_depth: usize,
    cancel: Arc<dyn CancelToken>,
    on_event: &(dyn Fn(AgentEvent) + Send + Sync),
) -> synthia_tool::ToolOutput {
    let runner: Arc<dyn CommandRunner> = Arc::new(StdCommandRunner::new());
    run_subagent_with_runner(
        agent,
        spec,
        parent_session_id,
        input_depth,
        cancel,
        on_event,
        &runner,
    )
    .await
}

/// Like [`run_subagent`] but with an injectable
/// [`CommandRunner`]. Production callers use [`run_subagent`]
/// (which delegates here with the real runner); tests pass a
/// recording fake so the gate + worktree lifecycle can be
/// observed without spawning real `git` / shell processes.
pub(crate) async fn run_subagent_with_runner(
    agent: Arc<dyn Agent>,
    spec: TaskSpec,
    parent_session_id: &str,
    input_depth: usize,
    cancel: Arc<dyn CancelToken>,
    on_event: &(dyn Fn(AgentEvent) + Send + Sync),
    runner: &Arc<dyn CommandRunner>,
) -> synthia_tool::ToolOutput {
    let child_depth = input_depth + 1;
    // A stable per-child trace id so downstream consumers (the
    // web frontend, session replay) can group a child's events
    // into one sub-agent bubble even when the parent delegated
    // to several peers in parallel. The agent runtime has no
    // session registry of its own (the server assigns session
    // ids), so we mint an opaque ULID here.
    let child_trace_id = ulid::Ulid::generate().to_string();
    let meta =
        AgentMeta::new(parent_session_id, child_trace_id.clone(), child_depth)
            .with_agent_name(Some(spec.agent.clone()));
    let mut input = AgentInput::text(spec.prompt.clone());
    input.subagent_depth = child_depth;

    // The first forwarded event carries the dispatch prompt on
    // its meta, so a session router can persist the prompt as
    // the child session's user turn without cloning it onto
    // every event of the run (see `AgentMeta::prompt`).
    let mut first_event = true;

    let mut stream = agent.run(input, cancel.clone()).await;
    let mut text = String::new();
    let mut end_reason = SessionEndReason::Completed;
    while let Some(event) = stream.next().await {
        match &event {
            AgentEvent::Model(ContentPart::Text(TextContent {
                text: t,
                ..
            })) => {
                if !t.is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(t);
                }
            }
            AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
                end_reason = reason.clone();
            }
            _ => {}
        }
        let event_meta = if first_event {
            first_event = false;
            meta.clone().with_prompt(Some(spec.prompt.clone()))
        } else {
            meta.clone()
        };
        on_event(AgentEvent::Agent(event_meta, Box::new(event)));
    }

    let child_text = if matches!(end_reason, SessionEndReason::Completed) {
        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    } else {
        None
    };

    let child_failed = !matches!(end_reason, SessionEndReason::Completed);

    // 1. Set up the worktree (if isolation is on). The worktree
    //    path becomes the gate's cwd and the only path the
    //    caller ever needs to know about.
    let mut worktree: Option<WorktreeInfo> = None;
    if spec.isolation.is_some() {
        let base_cwd = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        match worktree::create_worktree(&base_cwd, runner.as_ref()) {
            Ok(info) => worktree = Some(info),
            Err(err) => {
                return synthia_tool::ToolOutput::error(format!(
                    "failed to set up worktree for sub-agent \
                     `{}`: {err}",
                    spec.agent
                ));
            }
        }
    }

    let gate_cwd: std::path::PathBuf = worktree
        .as_ref()
        .map(|w| w.path.clone())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // 2. Run the gate (if attached). Pass ⇒ keep the child's
    //    text; anything else ⇒ wrap in a structured error.
    if let Some(gate_spec) = &spec.gate {
        let verdict = match run_gate(
            gate_spec,
            &gate_cwd,
            runner.as_ref(),
            cancel.clone(),
        ) {
            Ok(verdict) => verdict,
            Err(err) => {
                return synthia_tool::ToolOutput::error(format!(
                    "failed gate `{}`: {err}",
                    gate_spec.label()
                ));
            }
        };
        if verdict.is_failure() {
            let output = apply_gate_to_output(
                child_text.as_deref(),
                &verdict,
                gate_spec.label(),
            );
            // Best-effort cleanup of any worktree we created.
            if let Some(info) = worktree.take() {
                let _ = worktree::cleanup_worktree(
                    &std::env::current_dir().unwrap_or_default(),
                    &info,
                    &spec.agent,
                    runner.as_ref(),
                );
            }
            return output;
        }
    }

    // 3. Cleanup the worktree (if any). Dirty ⇒ branch-committed
    //    result is surfaced in the tool result only when no gate
    //    ran; a gate's verdict already produced the result, and
    //    appending a worktree report would muddy the message.
    let cleanup_report: Option<WorktreeCleanupResult> =
        if let Some(info) = worktree.take() {
            let base_cwd = std::env::current_dir().unwrap_or_default();
            let result = worktree::cleanup_worktree(
                &base_cwd,
                &info,
                &spec.agent,
                runner.as_ref(),
            );
            Some(result)
        } else {
            None
        };

    // 4. Compose the tool result. Order of preference:
    //    a. child failed ⇒ error message naming the reason.
    //    b. child produced text ⇒ return it (with an optional
    //       worktree branch summary appended when relevant).
    //    c. child produced no text + no gate ⇒ "produced no
    //       final output" error.
    if child_failed {
        return synthia_tool::ToolOutput::error(format!(
            "sub-agent `{}` ended before completing: {:?}\n\
             [subagent session: {child_trace_id}]",
            spec.agent, end_reason
        ));
    }

    match child_text {
        Some(text) => {
            let mut combined = text;
            if let Some(report) =
                cleanup_report.as_ref().filter(|r| r.has_changes)
                && let (Some(branch), Some(path)) =
                    (report.branch.as_ref(), report.path.as_ref())
            {
                combined.push_str(&format!(
                    "\n\n[worktree: branch `{branch}` at {path}, \
                     base {}]",
                    report.base_sha
                ));
            }
            combined.push_str(&session_line(&child_trace_id));
            synthia_tool::ToolOutput::text(combined)
        }
        None => synthia_tool::ToolOutput::error(format!(
            "sub-agent `{}` produced no final output{}",
            spec.agent,
            session_line(&child_trace_id)
        )),
    }
}

/// The machine-readable tail line every `task` result carries,
/// naming the child session the trace lives in. Consumers that
/// route child events into their own sessions (the server's
/// subagent router) keep the model's result and the UI's link
/// pointing at the same id.
fn session_line(child_session_id: &str) -> String {
    format!("\n\n[subagent session: {child_session_id}]")
}
