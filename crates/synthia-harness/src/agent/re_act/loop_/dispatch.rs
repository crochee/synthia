//! Shared seam helpers + the dispatch façade.
//!
//! The actual tool-dispatch pipeline lives in four focused
//! sibling modules. This file is intentionally small — it
//! holds the three things the dispatch seam as a whole
//! re-exports or shares:
//!
//! - [`WireToolResult`] — the wire payload a tool-result
//!   commit needs; produced by [`super::bucket`] and
//!   [`super::steps`] (length-stop failure), consumed by
//!   [`super::commit`]. Kept here because both producer and
//!   consumer live in this `loop_` sub-tree.
//! - [`lookup_execution_mode`] + [`tool_definitions`] — the
//!   loop's read-side helpers that the LLM call site needs
//!   before sampling. The projection is memoised on the
//!   registry version so the per-iteration cost is one hash
//!   lookup once the registry stops mutating.
//! - [`append_tool_result_hints`] + [`notify_error_hooks`] —
//!   the two steering seams that span the dispatch step but
//!   do not belong to any one bucket (hints ride on every
//!   successful result; error hooks fan out from every
//!   step's failure point).
//!
//! The panic guard the loop applies at every third-party seam lives one
//! level up, in [`crate::agent::re_act::catching_panics`] — `agent_impl`
//! uses it too, to cover a panic in the loop's own code.
//!
//! The dispatch pipeline itself is split by concern:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`super::bucket`] | The `execute_tools` orchestrator: Parallel / Sequential bucketing, semaphore concurrency, sequential aborts, commit-all-results |
//! | [`super::seams`]  | `execute_tool_inner`: the four steering seams (restriction, hook veto, guard pipeline, observation) for one call |
//! | [`super::route`]  | `dispatch_tool_call`: the three named routing steps (interceptor claim, registry lookup, stream drain) |
//! | [`super::commit`] | `commit_tool_result` + `last_assistant_text` |
//!
//! The orchestrators in [`super::bucket`] / [`super::seams`]
//! read as the step list each module's doc promises; per-step
//! control flow lives next to the step it implements.

use std::sync::Arc;

use synthia_core::Registry;
use synthia_provider::{ContentPart, Message, ToolDefinition};
use synthia_steering::{HookError, HookStage, run_hook};
use synthia_tool::ExecutionMode;

use super::ReActLoop;
use crate::{
    agent::re_act::{compose_tool_definitions, promoted_tool_names},
    events::WarningKind,
};

/// The wire payload for a single tool call result.
pub(in crate::agent::re_act) struct WireToolResult {
    pub(super) call_id: String,
    pub(super) tool_name: String,
    pub(super) content: Vec<ContentPart>,
    pub(super) is_error: bool,
    pub(super) metadata: serde_json::Map<String, serde_json::Value>,
    pub(super) truncated_by: Option<serde_json::Value>,
}

/// Resolve a tool's [`ExecutionMode`] from the registry. Sequential
/// is the conservative default for missing tools.
pub(in crate::agent::re_act) async fn lookup_execution_mode(
    registry: &synthia_tool::ToolRegistry,
    name: &str,
) -> ExecutionMode {
    match registry.get(name).await {
        Ok(Some(entry)) => entry.tool_instance().mode(),
        _ => ExecutionMode::Sequential,
    }
}

/// The model-facing tool list for this iteration — memoised.
pub(super) fn tool_definitions(
    this: &ReActLoop,
    messages: &[Message],
) -> Arc<Vec<ToolDefinition>> {
    let version = this.tool_registry.version();
    let descriptors = this.tool_registry.descriptors_cached();
    let called = promoted_tool_names(&descriptors, messages);
    let promoted: Vec<String> = {
        let mut names: Vec<String> = called.iter().cloned().collect();
        names.sort_unstable();
        names
    };

    {
        let memo = this.tool_projection.lock();
        if let Some(memo) = memo.as_ref()
            && memo.registry_version == version
            && memo.promoted == promoted
        {
            return Arc::clone(&memo.defs);
        }
    }

    let defs = Arc::new(compose_tool_definitions(
        &this.tool_registry,
        this.tool_surface.as_deref(),
        this.tool_restriction.as_deref(),
        &this.interceptors,
        &called,
    ));
    let mut memo = this.tool_projection.lock();
    *memo = Some(super::ToolProjectionMemo {
        registry_version: version,
        promoted,
        defs: Arc::clone(&defs),
    });
    defs
}

/// Append every triggered `AppendToToolResult`-point hint to this
/// tool's committed content.
pub(super) fn append_tool_result_hints(
    this: &ReActLoop,
    tool_name: &str,
    state: &synthia_context::AgentState,
    mut content: String,
) -> String {
    use synthia_steering::InjectionPoint;
    for hint in &this.steering.hints {
        if !matches!(
            hint.injection_point(),
            InjectionPoint::AppendToToolResult { .. }
        ) {
            continue;
        }
        let matches_tool = matches!(
            hint.injection_point(),
            InjectionPoint::AppendToToolResult { tool_name: ref name } if name == tool_name
        );
        if !matches_tool {
            continue;
        }
        // Same trait, same treatment as the pre-sample injection point
        // (`steps::inject_hints`): a panicking hint is skipped, not
        // allowed to end the run.
        let asked = crate::agent::re_act::catching_panics_sync(|| {
            hint.should_trigger(state).then(|| hint.generate(state))
        });
        match asked {
            Ok(Some(message)) => {
                content.push_str("\n\n[reminder] ");
                content.push_str(&message.content);
            }
            Ok(None) => {}
            Err(panic) => {
                tracing::warn!(
                    hint = hint.name(),
                    tool = tool_name,
                    panic = %panic,
                    "tool-result hint panicked; skipping it"
                );
            }
        }
    }
    content
}

/// Fan a failure reason out to every hook's `OnError` stage.
/// Hook-runner failures surface as warning events, never as
/// boot- or run-fatal errors.
pub(in crate::agent::re_act::loop_) async fn notify_error_hooks(
    this: &ReActLoop,
    stage: &'static str,
    reason: &str,
) {
    for hook in &this.steering.hooks {
        let reason_for_hook = reason.to_string();
        if let Err(err) = run_hook(&**hook, HookStage::OnError, move || {
            let hook = Arc::clone(hook);
            Box::pin(async move {
                hook.on_error(stage, &reason_for_hook).await;
                Ok::<_, HookError>(())
            })
        })
        .await
        {
            this.sink.system(crate::events::SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: err.to_string(),
                iteration: None,
            });
        }
    }
}
