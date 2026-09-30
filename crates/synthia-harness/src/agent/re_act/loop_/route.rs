//! Dispatch one guard-approved tool call to the right
//! destination.
//!
//! The route has exactly three named steps:
//!
//! 1. **Interceptor claim** — when an installed
//!    [`ToolInterceptor`] claims the tool's name, hand the
//!    call to it (it needs the run's cancel token, depth, and
//!    event emitter — the loop internals the registry path
//!    never sees). `None` falls through to the registry path.
//! 2. **Registry lookup** — note the call's file activity
//!    (compaction details), then resolve the entry through the
//!    [`ToolRegistry`]. Both lookup-failure arms return a
//!    synthesised error output the model should see.
//! 3. **Stream drain** — pump the tool's stream to completion,
//!    forwarding every `Progress` chunk as a
//!    `ToolProgress` event and stopping at the first `Result`
//!    (or cancellation, or stream end — each degenerates to a
//!    synthetic error).
//!
//! [`ToolInterceptor`]: crate::agent::ToolInterceptor
//! [`ToolRegistry`]: synthia_tool::ToolRegistry

use std::sync::Arc;

use futures::StreamExt;
use synthia_core::Registry;
use synthia_provider::ToolUse;
use synthia_tool::{Context, StreamOutput, ToolOutput};
use tracing::{debug, warn};

use super::{ReActLoop, seams::Verdict};
use crate::{
    agent::{interceptor::InterceptorCall, re_act::touched_target},
    events::SystemEvent,
};

/// Dispatch one (guard-approved) tool call: interceptors for
/// claimed names, registry stream otherwise.
///
/// Three named steps, each in its own helper below —
/// interceptor claim, registry lookup + touched-file note,
/// stream drain. The orchestrator reads as that list.
///
/// The registry path runs under
/// [`catch_unwind`](std::panic::catch_unwind): a tool is third-party
/// code, and a panic inside it must not unwind the run's detached task.
/// Without this the loop's task would die mid-unwind, the event sender
/// would drop, and the caller would see the stream end with **no**
/// [`SessionEnded`](crate::events::SystemEvent::SessionEnded) — unable
/// to tell a finished session from a dropped connection. The registry's
/// own dispatcher converts a panic into an error `Result` for the same
/// reason; this is the same conversion, since the loop drives the stream
/// itself rather than going through the registry.
///
/// Both branches are guarded, not just the registry one: the
/// interceptor is checked *first* and returns before this path is
/// reached, and a plugin (the delegator spawns whole subagents) can
/// panic exactly as a tool can.
pub(super) async fn dispatch_tool_call(
    this: &ReActLoop,
    call: &ToolUse,
    tool_name: &str,
    call_id: &str,
) -> ToolOutput {
    // Synthetic tools first.
    if let Some(output) = try_interceptor(this, call, tool_name).await {
        return output;
    }

    // R29-Phase-K: note file activity, then resolve the entry.
    let entry =
        match lookup_tool_entry(this, &call.input, tool_name, call_id).await {
            Verdict::Allowed(entry) => entry,
            Verdict::Denied(output) => return output,
        };

    // A panic during a tool call would unwind this task and leave the
    // run unreported; see [`catching_panics`] for why that matters.
    let ctx = Context::new(String::new(), this.workspace_root.clone());
    let tool = entry.tool_instance();
    let ran = crate::agent::re_act::catching_panics(drain_one_tool(
        this, tool, call, &ctx, tool_name, call_id,
    ))
    .await;

    match ran {
        Ok(output) => output,
        Err(message) => {
            warn!(
                tool_name = %tool_name,
                tool_use_id = %call_id,
                panic = %message,
                "dispatch_tool_call: tool panicked; reporting it as a \
                 tool error so the run can still end normally"
            );
            ToolOutput::error(format!(
                "tool `{tool_name}` panicked during execution: {message}"
            ))
        }
    }
}

/// Build the tool's stream and drain it. Split out so
/// [`dispatch_tool_call`] can wrap the whole thing — construction
/// *and* polling, either of which may panic — in one `catch_unwind`.
async fn drain_one_tool(
    this: &ReActLoop,
    tool: Arc<dyn synthia_tool::Tool>,
    call: &ToolUse,
    ctx: &Context,
    tool_name: &str,
    call_id: &str,
) -> ToolOutput {
    let mut stream = tool.stream(call.input.clone(), ctx);
    drain_tool_stream(this, &mut stream, tool_name, call_id).await
}

/// Step 1 of [`dispatch_tool_call`]: when an installed
/// interceptor claims the tool's name, hand the call to it
/// (it needs the run's cancel token, depth, and event
/// emitter — the loop internals the registry path never
/// sees). `None` falls through to the registry path.
async fn try_interceptor(
    this: &ReActLoop,
    call: &ToolUse,
    tool_name: &str,
) -> Option<ToolOutput> {
    let interceptor = this.interceptors.iter().find(|i| i.claims(tool_name))?;
    let cancel = Arc::clone(&this.cancel);
    let depth = this.depth;
    let emit: &(dyn Fn(crate::events::AgentEvent) + Send + Sync) = &|event| {
        let _ = this.sink.tx.unbounded_send(event);
    };
    let intercepted = InterceptorCall {
        call,
        cancel,
        depth,
        emit,
    };
    // An interceptor is a plugin (the delegator spawns whole subagents),
    // so it is exactly as capable of panicking as a tool is.
    Some(
        match crate::agent::re_act::catching_panics(
            interceptor.execute(intercepted),
        )
        .await
        {
            Ok(output) => output,
            Err(message) => {
                warn!(
                    tool_name = %tool_name,
                    panic = %message,
                    "dispatch_tool_call: interceptor panicked; reporting \
                     it as a tool error so the run can still end normally"
                );
                ToolOutput::error(format!(
                    "tool `{tool_name}` panicked during execution: \
                     interceptor: {message}"
                ))
            }
        },
    )
}

/// Step 2 of [`dispatch_tool_call`]: note the call's file
/// activity (R29-Phase-K compaction details), then resolve
/// the registry entry. The lookup-failure arms return a
/// synthesised error output the model should see; that includes
/// `is_hidden`, which the registry's own dispatcher
/// ([`ToolRegistry::run_stream`](synthia_tool::ToolRegistry::run_stream))
/// refuses too — a hidden tool must not become callable just
/// because a model guessed its name.
async fn lookup_tool_entry(
    this: &ReActLoop,
    input: &serde_json::Value,
    tool_name: &str,
    call_id: &str,
) -> Verdict<synthia_tool::ToolEntry> {
    if let Some((read, write)) = touched_target(tool_name, input) {
        let mut guard = this.touched.lock();
        if let Some(p) = read {
            guard.read_files.push(p);
        }
        if let Some(p) = write {
            guard.modified_files.push(p);
        }
    }
    match this.tool_registry.get(tool_name).await {
        Ok(Some(e)) if e.is_hidden() => {
            warn!(
                tool_name = %tool_name,
                tool_use_id = %call_id,
                "dispatch_tool_call: refusing a call to a hidden tool"
            );
            Verdict::Denied(ToolOutput::error(format!(
                "tool not found: {tool_name}"
            )))
        }
        Ok(Some(e)) => Verdict::Allowed(e),
        Ok(None) => {
            warn!(
                tool_name = %tool_name,
                tool_use_id = %call_id,
                "dispatch_tool_call: tool not found in registry"
            );
            Verdict::Denied(ToolOutput::error(format!(
                "tool not found: {tool_name}"
            )))
        }
        Err(e) => {
            warn!(
                tool_name = %tool_name,
                tool_use_id = %call_id,
                error = %e,
                "dispatch_tool_call: registry lookup error"
            );
            Verdict::Denied(ToolOutput::error(format!("registry error: {e}")))
        }
    }
}

/// Step 3 of [`dispatch_tool_call`]: pump the tool's stream
/// to completion, forwarding every Progress chunk as a
/// `ToolProgress` event and stopping at the first `Result`
/// (or cancellation, or stream end — each degenerates to a
/// synthetic error).
async fn drain_tool_stream<S>(
    this: &ReActLoop,
    stream: &mut S,
    tool_name: &str,
    call_id: &str,
) -> ToolOutput
where
    S: futures::Stream<Item = StreamOutput> + Unpin,
{
    let mut final_output: Option<ToolOutput> = None;
    loop {
        if this.cancel.is_cancelled() {
            debug!(
                tool_name = %tool_name,
                tool_use_id = %call_id,
                "dispatch_tool_call: cancelled mid-stream"
            );
            break;
        }
        match StreamExt::next(stream).await {
            Some(StreamOutput::Progress(output)) => {
                this.sink.system(SystemEvent::ToolProgress {
                    tool_name: tool_name.to_string(),
                    call_id: call_id.to_string(),
                    output,
                });
            }
            Some(StreamOutput::Result(output)) => {
                final_output = Some(output);
                break;
            }
            None => break,
        }
    }
    let result = final_output.unwrap_or_else(|| {
        ToolOutput::error("tool stream closed without producing a Result")
    });
    debug!(
        tool_name = %tool_name,
        tool_use_id = %call_id,
        is_error = result.is_error.unwrap_or(false),
        "dispatch_tool_call: tool stream completed"
    );
    result
}
