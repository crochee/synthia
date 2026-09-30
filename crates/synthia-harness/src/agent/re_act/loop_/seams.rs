//! Single-tool execution: the four steering seams in order.
//!
//! [`execute_tool_inner`] is the seam orchestrator. One call to
//! a model-returned [`ToolUse`] travels through:
//!
//! 1. The agent's own [`ToolRestriction`] (R58).
//! 2. Every hook's `BeforeToolExecute` vote.
//! 3. The guard pipeline (may rewrite the call).
//! 4. The actual dispatch through
//!    [`super::route::dispatch_tool_call`].
//! 5. Tracker observation + every hook's `AfterToolExecute`
//!    observation (fires regardless of outcome shape).
//!
//! Seams 0, 1, 2, 3 live in this module; seam 4 (the dispatch)
//! lives in [`super::route`]. The split keeps the
//! `tool called once, outcomes emitted` contract local while
//! moving the registry / interceptor wiring elsewhere.

use std::{sync::Arc, time::Instant};

use synthia_context::AgentState;
use synthia_provider::ToolUse;
use synthia_steering::{
    Action,
    GuardOutcome,
    HookAction,
    HookError,
    HookStage,
    run_guards,
    run_hook,
};
use synthia_tool::ToolOutput;
use tracing::{debug, warn};

use super::ReActLoop;
use crate::{
    agent::re_act::preview,
    events::{SystemEvent, WarningKind},
};

/// Run a single tool call to completion. Does NOT commit to
/// `messages` — the caller decides commit order.
///
/// The body is the four steering seams in order — restriction,
/// hook veto, guard pipeline, observation — each in its own
/// helper below, then the dispatch itself. The orchestrator
/// reads as the seam list; each seam's control flow lives
/// next to the seam it implements.
pub(super) async fn execute_tool_inner(
    this: &ReActLoop,
    call: &ToolUse,
    state: &AgentState,
) -> ToolOutput {
    let tool_name = call.name.clone();
    let call_id = call.id.clone();
    debug!(
        tool_name = %tool_name,
        tool_use_id = %call_id,
        "execute_tool_inner: invoking tool"
    );

    // R58 seam 0 + steering seam 1 — the agent's tool
    // restriction, then the hooks' BeforeToolExecute veto.
    if let Some(denied) =
        pre_execution_veto(this, call, &tool_name, &call_id, state).await
    {
        return denied;
    }

    // Steering seam 2 — guard pipeline (may rewrite the call).
    let effective_call =
        match apply_guard_pipeline(this, call, &tool_name, &call_id, state)
            .await
        {
            Verdict::Allowed(rewritten) => rewritten,
            Verdict::Denied(denied) => return denied,
        };

    let started = Instant::now();
    let result = this
        .dispatch_tool_call(&effective_call, &effective_call.name, &call_id)
        .await;
    let elapsed = started.elapsed();

    // Steering seam 3 — observation: tracker boundary and
    // after-execute hooks fire regardless of outcome shape.
    // Consumer-supplied (`Steering`); isolated like the other surfaces.
    if let Err(panic) = crate::agent::re_act::catching_panics_sync(|| {
        this.steering.tracker.on_tool_call(
            &tool_name,
            &effective_call.input,
            state,
        );
    }) {
        warn!(panic = %panic, "tracker.on_tool_call panicked");
    }
    notify_after_hooks(this, &tool_name, &result, elapsed).await;
    result
}

/// Commit a refused tool call: the tracker sees it, and the model gets
/// the error that says why.
///
/// The three veto seams (restriction, hook, guard) converge here, which
/// is what makes "a denied call is always recorded and always explained"
/// one rule instead of copies of the same tail. The hook seam does more
/// than these two statements — it also fans out `notify_error_hooks` —
/// so it calls this helper *before* that await, keeping the record ahead
/// of the hook notification exactly as it was written inline.
fn denied(
    this: &ReActLoop,
    tool_name: &str,
    call: &ToolUse,
    state: &AgentState,
    message: String,
) -> ToolOutput {
    this.steering
        .tracker
        .on_tool_call(tool_name, &call.input, state);
    ToolOutput::error(message)
}

/// Seams 0+1 of [`execute_tool_inner`]: the agent's own tool
/// restriction (R58), then every hook's `BeforeToolExecute`
/// vote. Returns the error output the caller should commit
/// when either seam denies the call; `None` lets it through.
async fn pre_execution_veto(
    this: &ReActLoop,
    call: &ToolUse,
    tool_name: &str,
    call_id: &str,
    state: &AgentState,
) -> Option<ToolOutput> {
    // R58 seam 0 — the agent's own tool restriction.
    if let Some(restriction) = this.tool_restriction.as_deref()
        && !restriction.is_relevant(tool_name, false)
    {
        warn!(
            tool_name = %tool_name,
            tool_use_id = %call_id,
            "execute_tool_inner: tool denied by the agent's restriction"
        );
        this.sink.system(SystemEvent::Warning {
            kind: WarningKind::Guard,
            message: format!(
                "tool restriction denied `{tool_name}`: it is not \
                 available to this agent",
            ),
            iteration: Some(state.iteration_count),
        });
        return Some(denied(
            this,
            tool_name,
            call,
            state,
            format!(
                "[denied by tool restriction] `{tool_name}` is not \
                 available to this agent; use one of the tools you were \
                 given, or answer from what you have.",
            ),
        ));
    }

    // Steering seam 1 — hooks veto.
    for hook in &this.steering.hooks {
        let verdict = hook_before_verdict(this, hook, tool_name, call).await;
        if let HookAction::Block(reason) = verdict {
            warn!(
                hook = hook.name(),
                tool_name = %tool_name,
                tool_use_id = %call_id,
                "execute_tool_inner: hook blocked tool execution"
            );
            // Record first, then notify — the order the inline version
            // had. A hook that inspects tracking state on error sees the
            // denial already recorded.
            let committed = denied(
                this,
                tool_name,
                call,
                state,
                format!("[blocked by hook `{}`] {reason}", hook.name()),
            );
            super::dispatch::notify_error_hooks(this, "tool_veto", &reason)
                .await;
            return Some(committed);
        }
    }
    None
}

/// One hook's `BeforeToolExecute` vote, with the double
/// `Result` (runner error, hook error) flattened to a
/// [`HookAction`] — a hook that fails is a `Continue`, not a
/// silent veto, and the failure is surfaced as a warning
/// event.
async fn hook_before_verdict(
    this: &ReActLoop,
    hook: &Arc<dyn synthia_steering::AgentHook>,
    tool_name: &str,
    call: &ToolUse,
) -> HookAction {
    let tool_name_for_hook = tool_name.to_string();
    let input_for_hook = call.input.clone();
    let verdict = run_hook(&**hook, HookStage::BeforeToolExecute, move || {
        let hook = Arc::clone(hook);
        let tool_name_for_hook = tool_name_for_hook.clone();
        let input_for_hook = input_for_hook.clone();
        Box::pin(async move {
            Ok(hook
                .before_tool_execute(&tool_name_for_hook, &input_for_hook)
                .await)
        })
    })
    .await;
    match verdict {
        Ok(Ok(v)) => v,
        Ok(Err(())) => HookAction::Continue,
        Err(err) => {
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: err.to_string(),
                iteration: None,
            });
            HookAction::Continue
        }
    }
}

/// Seam 2 of [`execute_tool_inner`]: the guard pipeline.
/// `Allowed` carries the (possibly rewritten) call;
/// `Denied` carries the error output to commit instead.
async fn apply_guard_pipeline(
    this: &ReActLoop,
    call: &ToolUse,
    tool_name: &str,
    call_id: &str,
    state: &AgentState,
) -> Verdict<ToolUse> {
    if this.steering.guards.is_empty() {
        return Verdict::Allowed(call.clone());
    }
    let action = Action::from_tool_use(call);
    match run_guards(&this.steering.guards, &action, state) {
        GuardOutcome::Allowed { action } => {
            Verdict::Allowed(action.apply_to(call))
        }
        GuardOutcome::Denied {
            guard,
            reason,
            severity,
        } => {
            warn!(
                guard = %guard,
                severity = severity.as_str(),
                tool_name = %tool_name,
                tool_use_id = %call_id,
                action = %action.summary(),
                "execute_tool_inner: guard denied action"
            );
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Guard,
                message: format!(
                    "guard `{guard}` ({}) denied `{}`: {reason}",
                    severity.as_str(),
                    action.summary(),
                ),
                iteration: Some(state.iteration_count),
            });
            Verdict::Denied(denied(
                this,
                tool_name,
                call,
                state,
                format!(
                    "[blocked by guard `{guard}` ({})] {reason}",
                    severity.as_str(),
                ),
            ))
        }
    }
}

/// Seam 3 of [`execute_tool_inner`]: every hook's
/// `AfterToolExecute` observation, with the result's textual
/// preview. Fires regardless of outcome shape.
async fn notify_after_hooks(
    this: &ReActLoop,
    tool_name: &str,
    result: &ToolOutput,
    elapsed: std::time::Duration,
) {
    let is_error = result.is_error.unwrap_or(false);
    let preview_text = preview(&result.content);
    for hook in &this.steering.hooks {
        let preview_for_hook = preview_text.clone();
        let tool_name = tool_name.to_string();
        if let Err(err) =
            run_hook(&**hook, HookStage::AfterToolExecute, move || {
                let hook = Arc::clone(hook);
                Box::pin(async move {
                    hook.after_tool_execute(
                        &tool_name,
                        &preview_for_hook,
                        is_error,
                        elapsed,
                    )
                    .await;
                    Ok::<_, HookError>(())
                })
            })
            .await
        {
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: err.to_string(),
                iteration: None,
            });
        }
    }
}

/// "Allowed (carrying a value) or denied (carrying the error
/// output the model should see)" — the return shape the
/// guard pipeline and the registry lookup share. A dedicated
/// enum rather than `Result<T, ToolOutput>` because
/// `ToolOutput` is large and the denial is not an error the
/// caller handles, it is a value the caller commits.
pub(in crate::agent::re_act) enum Verdict<T> {
    Allowed(T),
    Denied(ToolOutput),
}
