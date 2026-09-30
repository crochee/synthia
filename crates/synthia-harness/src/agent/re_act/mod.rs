//! The canonical ReAct loop, the public agent type, and the
//! default reasoning strategy.
//!
//! ## Layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `agent` | `ReActAgent` factory + the `Agent` trait impl + every `with_*` builder |
//! | `strategy` | `ReActStrategy` — the default `ReasoningStrategy` the harness runs |
//! | `prompt` | `DEFAULT_SYSTEM_PROMPT` — the base instructions template |
//! | `loop_` | The state-machine driver for one session: prepare → sample → execute → finalize |
//! | `stream` | Provider chunk → `AgentEvent` translation; the only module that knows chunk shapes |
//!
//! ## The harness contract
//!
//! With **zero** interceptors, the loop has no `task` tool, no
//! delegation, no multi-agent machinery — it is a single-loop
//! harness. Every external capability is composed in through the
//! registry, the strategy seam, or [`ToolInterceptor`].
//!
//! [`Agent`]: crate::agent::Agent
//! [`ReasoningStrategy`]: crate::agent::ReasoningStrategy
//! [`ToolInterceptor`]: crate::agent::ToolInterceptor

// --- Constants visible to the rest of the agent crate ---------------------

/// Default per-run iteration cap when no `max_iterations` is
/// configured. Industry-aligned with the Anthropic / OpenAI Agents
/// SDK defaults; override via
/// [`ReActAgent::with_max_iterations`].
pub const DEFAULT_MAX_ITERATIONS: usize = 25;

// --- Submodules ----------------------------------------------------------

mod agent;
mod agent_descriptor;
mod agent_impl;
mod agent_runtime;
mod loop_;
mod prompt;
mod strategy;
mod stream;

// --- Module-private utilities, exposed to sibling submodules and tests ---

use std::{collections::HashSet, path::PathBuf};

pub use agent::ReActAgent;
use futures::FutureExt;
pub(crate) use loop_::SampleOutcome;
pub use prompt::DEFAULT_SYSTEM_PROMPT;
use serde_json::Value;
pub use strategy::ReActStrategy;
pub(crate) use stream::ChunkState;
#[cfg(test)]
pub(crate) use synthia_provider::ToolUse;
use synthia_provider::{Content, ContentPart, Message, Role, TextContent};

/// Await `fut`, converting a panic into `Err(message)`.
///
/// The loop calls several seams that are third-party code by
/// construction — the [`ModelProvider`](synthia_provider::ModelProvider),
/// the [`Tool`](synthia_tool::Tool), the
/// [`ToolInterceptor`](crate::agent::ToolInterceptor) plugin, the
/// [`ContextManager`](synthia_context::ContextManager), and the
/// [`RunInbox`](crate::agent::RunInbox). A panic in any of them unwinds
/// the run's detached task: the event sender drops with it, and the
/// caller watches the stream end with **no**
/// [`SessionEnded`](crate::events::SystemEvent::SessionEnded), unable to
/// tell a finished session from a dropped connection. Converting the
/// panic into a value the loop already knows how to report keeps the
/// run's own bookkeeping (error hooks, warnings, terminal reason) intact.
///
/// What a caught panic *becomes* is the caller's decision, because the
/// seams differ in what they can safely carry on with:
/// [`Tool`](synthia_tool::Tool) conversions land in the tool result (the
/// model can self-correct), `Hint`/`Tracker` and `RunInbox` panics skip
/// only their own work, and a `ContextManager` panic ends the run
/// **reported** — `prepare` rewrites the message list in place, so a
/// half-truncated transcript is not something to keep sampling from. A
/// `ModelProvider` panic necessarily ends it, there being no response to
/// continue with.
///
/// `AssertUnwindSafe` is sound at these call sites: each is a
/// self-contained piece of work whose inputs are passed by value or by
/// shared reference, and a caught panic produces a value the loop
/// reports rather than resuming from the panic point.
pub(in crate::agent::re_act) async fn catching_panics<F, T>(
    fut: F,
) -> Result<T, String>
where
    F: Future<Output = T>,
{
    match std::panic::AssertUnwindSafe(fut).catch_unwind().await {
        Ok(value) => Ok(value),
        // By value: see `synthia_core::panic_message`'s docs for why a
        // reference parameter would let the message be silently lost.
        Err(payload) => Err(synthia_core::panic_message(payload)),
    }
}

/// Run a **synchronous** closure, converting a panic into `Err(message)`.
///
/// The companion to [`catching_panics`] for the seams that are not
/// `async`: `steering`'s `Hint` and `Tracker` traits are plain methods,
/// so a consumer-supplied implementation must be isolated the same way
/// whether or not it happens to be awaited. Named distinctly rather than
/// overloading `catching_panics`, because the two differ in calling
/// convention — one needs `.await`, the other must not — and a reader
/// should be able to tell which is in play at the call site.
pub(in crate::agent::re_act) fn catching_panics_sync<F, T>(
    f: F,
) -> Result<T, String>
where
    F: FnOnce() -> T,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => Ok(value),
        Err(payload) => Err(synthia_core::panic_message(payload)),
    }
}

#[cfg(test)]
#[allow(unused_imports)] // re-exported for tests.rs via `use super::*`
pub(crate) use crate::{
    Agent,
    agent::{
        descriptor::AgentDescriptor,
        strategy::{EventSink, KNOWN_STRATEGY_NAMES},
    },
    events::{
        AgentEvent,
        AgentMeta,
        AgentOutput,
        SessionEndReason,
        SteeringSource,
        SystemEvent,
        WarningKind,
    },
    input::AgentInput,
    prompt::PromptContext,
};

/// True when a provider stop reason means the assistant output
/// was cut off by the output token limit, so tool-call arguments
/// in that turn may be silently truncated.
///
/// Providers spell the reason differently: OpenAI-compatible
/// APIs report `"length"`, Anthropic reports `"max_tokens"`.
/// Matching is case-insensitive so gateway normalizations do not
/// slip past the guard. Everything else — `"end_turn"`,
/// `"tool_use"`, `None`, … — is a normal stop.
pub(crate) fn is_length_stop(stop_reason: &Option<String>) -> bool {
    stop_reason.as_deref().is_some_and(|reason| {
        matches!(
            reason.to_ascii_lowercase().as_str(),
            "length" | "max_tokens"
        )
    })
}

/// Project one tool call onto the `(read_path, write_path)`
/// pair the compaction-details accumulator tracks.
///
/// Only the deterministic builtins participate. `read` reports
/// its `path` argument as a read; `write` reports its `path` as a
/// modification. Anything else yields `None` — shell commands in
/// particular, because inferring the files a `sh -c` touched is
/// a guess, and a wrong claim in the summariser prompt is worse
/// than an absent one.
pub(crate) fn touched_target(
    tool_name: &str,
    input: &Value,
) -> Option<(Option<PathBuf>, Option<PathBuf>)> {
    let path = input.get("path").and_then(Value::as_str).map(PathBuf::from);
    match tool_name {
        "read" => Some((path, None)),
        "write" => Some((None, path)),
        _ => None,
    }
}

/// Plain-text view of the run's input (text parts of
/// [`crate::input::AgentInput::content`]), for the `on_agent_start`
/// hook.
pub(crate) fn input_text(input: &crate::input::AgentInput) -> String {
    input
        .content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Append `addition` to the leading system message (creating one
/// when absent). Used for `SystemPrompt`-point hints.
pub(crate) fn append_to_system_prompt(
    messages: &mut Vec<Message>,
    addition: &str,
) {
    if let Some(message) = messages.first_mut()
        && matches!(message.role, Role::System)
    {
        let mut text = message.content.extract_text().unwrap_or_default();
        text.push_str("\n\n");
        text.push_str(addition);
        message.content = Content::text(text);
        return;
    }
    messages.insert(0, Message::system(addition.to_string()));
}

/// Textual projection of a tool result: text parts joined with
/// newlines, non-text parts skipped.
pub(crate) fn parts_text(content: &[ContentPart]) -> String {
    content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render up to 160 chars of a tool result for the `after_tool_execute`
/// hook preview. Non-text parts are dropped.
pub(crate) fn preview(content: &[ContentPart]) -> String {
    let text = parts_text(content);
    const MAX: usize = 160;
    if text.chars().count() <= MAX {
        text
    } else {
        let truncated: String = text.chars().take(MAX).collect();
        format!("{truncated}…")
    }
}

/// Replace every text part in `content` with a single part
/// carrying `text`, keeping the position of the first text part
/// and all non-text parts.
pub(crate) fn replace_text_parts(content: &mut Vec<ContentPart>, text: String) {
    let insert_at = content
        .iter()
        .position(|part| matches!(part, ContentPart::Text(_)))
        .unwrap_or(content.len());
    content.retain(|part| !matches!(part, ContentPart::Text(_)));
    content.insert(
        insert_at.min(content.len()),
        ContentPart::Text(TextContent {
            text,
            cache_control: None,
        }),
    );
}

/// Compose the model-facing tool list for one request.
///
/// [`synthia_tool::project_tool_definitions`] is the single
/// projection (exposure + transcript-driven `Deferred` promotion);
/// `surface` adds the deployment-level visible-name filter on top —
/// `None` advertises every non-hidden tool, exactly as before the
/// policy existed. Interceptor definitions are appended on top: they
/// are never registered in the registry (they need the loop's event
/// sink + depth bookkeeping), so they bypass both the projection and
/// the surface filter — but not the restriction.
pub(crate) fn compose_tool_definitions(
    registry: &synthia_tool::ToolRegistry,
    surface: Option<&synthia_tool::ToolSurfacePolicy>,
    restriction: Option<&synthia_tool::ToolRestriction>,
    interceptors: &[std::sync::Arc<dyn super::interceptor::ToolInterceptor>],
    called: &HashSet<String>,
) -> Vec<synthia_provider::ToolDefinition> {
    // The self-management MCP tools (`mcp__self__*`) are a server-
    // facing surface (operators reach them via the MCP client), not
    // a model-facing one. Filtering them out here keeps them out of
    // every descriptor list the agent loop assembles, regardless of
    // what the deployment's `[tools]` config says: the policy applies
    // to operator-facing plugins, not the server's own management
    // API. A regression that accidentally re-registered them as
    // visible to the model would silently push "manage your own
    // server" tools at the LLM, which is exactly the surface the
    // boot hides.
    let descriptors: Vec<synthia_tool::ToolDescriptor> = registry
        .descriptors_cached()
        .iter()
        .filter(|d| !d.name.starts_with("mcp__self__"))
        .cloned()
        .collect();
    let visible: Option<HashSet<String>> = {
        let from_surface = surface
            .map(|policy| policy.visible_tool_names(&descriptors))
            .unwrap_or_else(|| {
                descriptors.iter().map(|d| d.name.clone()).collect()
            });
        match restriction {
            Some(restriction) => Some(
                from_surface
                    .into_iter()
                    .filter(|name| restriction.is_relevant(name, false))
                    .collect(),
            ),
            None if surface.is_some() => Some(from_surface),
            None => None,
        }
    };
    let mut defs = synthia_tool::project_tool_definitions(
        &descriptors,
        called,
        visible.as_ref(),
    );
    for def in interceptors
        .iter()
        .flat_map(|interceptor| interceptor.definitions())
    {
        let allowed = restriction
            .map(|restriction| restriction.is_relevant(&def.name, false))
            .unwrap_or(true);
        if allowed {
            defs.push(def);
        }
    }
    defs
}

/// The tool names the transcript has already called, for `Deferred`
/// promotion. Skips the scan entirely when no `Deferred` tool is
/// registered (the common case).
pub(crate) fn promoted_tool_names(
    descriptors: &[synthia_tool::ToolDescriptor],
    messages: &[Message],
) -> HashSet<String> {
    if descriptors
        .iter()
        .any(|d| d.exposure == synthia_tool::ToolExposure::Deferred)
    {
        synthia_tool::called_tool_names(messages)
    } else {
        HashSet::new()
    }
}

// --- Re-exports the loop needs from this module's siblings --------------

// --- Tests ---------------------------------------------------------------

#[cfg(test)]
mod tests;
