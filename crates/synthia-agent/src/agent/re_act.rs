//! [`ReActAgent`] — concrete [`Agent`] implementation. The whole
//! ReAct loop lives in this module so an `impl Agent` is fully
//! self-contained: no internal bridges, no `pub(crate)` callback
//! helpers.
//!
//! ## Object-oriented layout
//!
//! | Type | Responsibility |
//! |---|---|
//! | [`ReActAgent`] | Factory + [`Agent`] trait impl. Holds shared deps. |
//! | [`ReActLoop`]  | One session's loop driver. Bound to [`ReActAgent`] deps. |
//! | [`SampleOutcome`] | Result of a single LLM sampling pass. |
//! | [`StreamSink`] | Thin wrapper over the mpsc sender with typed `emit_*` helpers. |
//!
//! ## Loop shape (5 explicit steps)
//!
//! 1. [`ReActLoop::prepare`] — seed `messages` from input.
//!    Assembles the system prompt via
//!    [`crate::prompt::PromptContext::assemble`].
//! 2. [`ReActLoop::sample_once`] — call the provider, translate stream chunks
//!    into [`AgentEvent`]s, accumulate into a [`SampleOutcome`].
//! 3. [`ReActLoop::commit_assistant`] — append the assistant turn to history.
//! 4. [`ReActLoop::execute_tool`] — run one tool call, push tool result.
//! 5. [`ReActLoop::finalize`] — emit terminal events, return [`AgentOutput`].
//!
//! Cancellation is checked between every step and inside the streaming
//! callback.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde_json::Value;
use synthia_core::{Registry, registry::RegistryItem};
use synthia_provider::{
    CompletionRequest,
    Content,
    ContentPart,
    Message,
    Role,
    StreamChunk,
    TextContent,
    ToolChoice,
    ToolDefinition,
    ToolResult,
    ToolUse,
    traits::ModelProvider,
};
use synthia_tool::{Context, StreamOutput, ToolRegistry};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};

use super::{Agent, descriptor::AgentDescriptor};
use crate::{
    events::{
        AgentEvent,
        AgentOutput,
        SessionEndReason,
        SystemEvent,
        WarningKind,
    },
    input::AgentInput,
    prompt::{Environment, PromptContext},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Hard cap on ReAct iterations.
const MAX_ITERATIONS: usize = 25;

/// The wire payload for a single tool call result. Bundled
/// here so [`ReActAgent::commit_tool_result`] doesn't grow
/// an unwieldy 8-argument signature now that `metadata` /
/// `truncated_by` are forwarded through.
struct WireToolResult {
    call_id: String,
    tool_name: String,
    content: Vec<ContentPart>,
    is_error: bool,
    metadata: serde_json::Map<String, serde_json::Value>,
    truncated_by: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// ReActAgent — factory + Agent trait impl
// ---------------------------------------------------------------------------

/// Concrete [`Agent`] implementing the ReAct loop.
///
/// Holds the shared dependencies (provider, tool registry) used by every
/// session. Each call to [`Agent::run`] spawns a [`ReActLoop`] bound to
/// this agent's fields plus the per-call `input` and `cancel`.
pub struct ReActAgent {
    descriptor: AgentDescriptor,
    provider: Arc<dyn ModelProvider>,
    tool_registry: Arc<ToolRegistry>,
    /// Working directory handed to built-in tools as `Context::working_dir`.
    /// Replaces the previous hard-coded `/tmp` which forced every
    /// `read_file` / `shell` invocation into the system temp dir.
    workspace_root: PathBuf,
    /// Manifest context injected into the system prompt: tool
    /// definitions, enabled skills, and registered peer agents.
    /// See [`crate::prompt::PromptContext`].
    ///
    /// Stored behind an `Arc` so the per-dispatch hot path inside
    /// [`Agent::run`] can move the manifest into the spawned
    /// `ReActLoop` without cloning every `String` and every
    /// `AgentDescriptor` field. The previous owned-on-self storage
    /// meant every chat dispatch deep-cloned the entire skills
    /// list + every peer-agent descriptor; with ~50 skills and
    /// ~10 peer agents that's tens of KB of heap + String allocs
    /// per turn for data that never mutates after boot.
    prompt_context: Arc<PromptContext>,
}

impl ReActAgent {
    /// Build a ReAct agent with explicit components.
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        Self::with_options(
            provider,
            tool_registry,
            PathBuf::new(),
            DEFAULT_SYSTEM_PROMPT.to_string(),
        )
    }

    /// Build a ReAct agent with full control over its prompt and
    /// working directory.
    pub fn with_options(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
        workspace_root: PathBuf,
        system_prompt: String,
    ) -> Self {
        let descriptor = AgentDescriptor {
            name: "agent".to_string(),
            // `display_name` is the human-readable label
            // surfaced to the model (in the `<identity>`
            // block) and to A2A clients (on the
            // `AgentCard`). Distinct from the programmatic
            // `name` ("agent"), which stays as the
            // registry / routing slug so existing user
            // configs (`config.agent.default = "agent"`)
            // and tests keep working unchanged.
            display_name: Some("Synthia".to_string()),
            description:
                "Default Synthia agent. Single-agent ReAct loop that uses \
                 tools to complete user tasks end-to-end."
                    .to_string(),
            kind: "react".to_string(),
            version: "1.0.0".to_string(),
            instructions: system_prompt,
            capabilities: vec![
                "tools".to_string(),
                "streaming".to_string(),
                "cancellation".to_string(),
            ],
            tools: Vec::new(),
            model_hint: None,
            handoffs: Vec::new(),
            handoff_hint: Some(
                "Use for general coding and tool-use tasks when no \
                 specialist agent is a better fit."
                    .to_string(),
            ),
            output_schema: None,
            owner: Some("synthia".to_string()),
            domain: Some("coding".to_string()),
            // The default persona stays because the prompt
            // assembler renders it via the `<identity>` block.
            // Kept short — the long-form behavioural guidance
            // lives in `DEFAULT_SYSTEM_PROMPT` (the base
            // instructions prepended verbatim before the
            // assembled sections).
            persona: Some(
                "You are Synthia, a pragmatic senior engineer \
                 working alongside the user."
                    .to_string(),
            ),
        };
        Self {
            descriptor,
            provider,
            tool_registry,
            workspace_root,
            prompt_context: Arc::new(PromptContext::default()),
        }
    }

    /// Build a ReAct agent that injects the given prompt context
    /// (skills, peer agents, tool manifest) into the assembled
    /// system prompt. Use this from the server layer once the
    /// registries are populated; the bare constructors leave
    /// `PromptContext` empty for back-compat.
    pub fn with_prompt_context(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
        workspace_root: PathBuf,
        system_prompt: String,
        prompt_context: Arc<PromptContext>,
    ) -> Self {
        let mut me = Self::with_options(
            provider,
            tool_registry,
            workspace_root,
            system_prompt,
        );
        me.prompt_context = prompt_context;
        me
    }

    /// Build a ReAct agent bound to an explicit [`AgentDescriptor`]
    /// and prompt manifest. Used by the run factory after the
    /// server-side run controller resolves the configured
    /// agent name into a descriptor. Overrides the legacy
    /// "use `system_prompt` as base instructions" behaviour.
    ///
    /// Note: this path bypasses [`Self::with_options`] on purpose
    /// so the caller's descriptor is installed verbatim instead
    /// of being shadowed by the default descriptor that
    /// `with_options` would construct first.
    pub fn with_descriptor(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
        workspace_root: PathBuf,
        descriptor: AgentDescriptor,
        prompt_context: Arc<PromptContext>,
    ) -> Self {
        Self {
            descriptor,
            provider,
            tool_registry,
            workspace_root,
            prompt_context,
        }
    }

    /// Replace the prompt context used to assemble the system
    /// prompt. Useful when registries are populated after
    /// construction.
    pub fn set_prompt_context(&mut self, ctx: PromptContext) {
        self.prompt_context = Arc::new(ctx);
    }

    /// Borrow the current prompt context.
    pub fn prompt_context(&self) -> &PromptContext {
        &self.prompt_context
    }

    /// Replace the descriptor used by the prompt assembler. Used
    /// by tests and by orchestration layers that compose
    /// descriptors dynamically (e.g. panel reassignment).
    pub fn descriptor_mut(&mut self, descriptor: AgentDescriptor) {
        self.descriptor = descriptor;
    }
}

/// Default system prompt injected into every conversation.
///
/// Kept short on purpose — provider-specific cache control and
/// longer templates should be assembled by the caller via
/// [`ReActAgent::with_options`].
///
/// The shape follows the same XML-delimited convention as
/// [`crate::prompt::PromptContext::assemble`] so the base
/// block reads consistently with the trailing `<identity>`,
/// `<env>`, `<available_skills>`, and `<available_agents>`
/// sections the assembler appends. Behavioural guidance is
/// distilled from the Grok Build and OpenCode default agent
/// prompts: action safety, tool preference, output style,
/// and an explicit "do what was asked; nothing more, nothing
/// less" stance.
pub const DEFAULT_SYSTEM_PROMPT: &str = "\
You are Synthia, an AI coding assistant. Your main goal is to complete \
the user's request thoroughly and accurately.

<action_safety>
Weigh each action by how easily it can be undone and how far its effects \
reach. Local, reversible work such as editing files and running tests is \
fine to do freely. Before executing any actions that are hard to reverse, \
reach shared external systems, or are otherwise risky or destructive, \
check with the user first.

Confirming is cheap; a mistaken action is not (such as lost work, messages \
you cannot unsend, deleted branches). For those cases, take the context, \
the action, and the user's instructions into account; by default, say \
what you plan to do and ask before doing it. Users can override that \
default — if they explicitly ask you to act more autonomously, you may \
proceed without confirmation, but still mind risks and consequences.

One approval is not a blank check. Approving something once (e.g. a git \
push) does not approve it in every later situation. Unless the user has \
authorized the action in advance, confirm with the user.

Examples of risky actions that warrant user confirmation:
- Destructive operations such as removing files or branches, dropping \
  database tables, killing processes, `rm -rf`, discarding uncommitted work
- Irreversible operations such as force-pushes (including overwriting \
  remote history), `git reset --hard`, amending commits already \
  published, removing or downgrading dependencies, changing CI/CD pipelines
- Actions others can see, or that change shared state: pushing code; \
  opening, closing, or commenting on PRs and issues; sending messages \
  (Slack, email, GitHub); posting to external services; changing shared \
  infrastructure or permissions

If you find unexpected state — unfamiliar files, branches, or \
configuration — investigate before deleting or overwriting; it may be \
the user's in-progress work.
</action_safety>

<tool_calling>
- Use specialized tools instead of bash commands whenever possible, as \
  this provides a better user experience. For file operations, prefer \
  dedicated file tools (read, edit, write) over `cat`/`sed`/`awk`/shell \
  echoes. Reserve shell tools exclusively for actual system commands \
  and terminal operations that require shell execution.
- NEVER use shell `echo`, `printf`, or other command-line tools to \
  communicate thoughts, explanations, or instructions to the user. \
  Output all communication directly in your response text instead.
- When multiple independent calls are safe to run in parallel, issue \
  them together in the same turn rather than serially.
</tool_calling>

<output_efficiency>
- Write like an excellent technical blog post — precise, well-structured, \
  and clear, in complete sentences. Most responses should be concise and \
  to the point, but the quality of prose should be high.
- Prefer simple, accessible language over dense technical jargon. Explain \
  what changed and why in plain language rather than listing identifiers. \
  Stay focused: avoid filler, repetition, over-the-top detail, and \
  tangents the user did not ask for.
- Keep final responses proportional to task complexity.
- Do what has been asked; nothing more, nothing less. When the task is \
  ambiguous, surface the ambiguity and ask one clarifying question \
  rather than guessing.
</output_efficiency>

<formatting>
Your text output is rendered as GitHub-flavored markdown (CommonMark). \
Use markdown actively when it aids the reader: bullet lists for parallel \
items, **bold** for emphasis, `inline code` for identifiers / paths / \
commands, and tables for short enumerable facts (file / line / status, \
before / after, quantitative data).
</formatting>";

#[async_trait]
impl Agent for ReActAgent {
    fn descriptor(&self) -> &AgentDescriptor {
        &self.descriptor
    }

    async fn run(
        &self,
        input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        let (tx, rx) = mpsc::unbounded_channel::<AgentEvent>();
        let provider = Arc::clone(&self.provider);
        let tool_registry = Arc::clone(&self.tool_registry);
        let workspace_root_clone = self.workspace_root.clone();
        let descriptor_clone = self.descriptor.clone();
        let prompt_context_arc = Arc::clone(&self.prompt_context);
        let cancel_for_task = Arc::clone(&cancel);

        tokio::spawn(async move {
            let loop_ = ReActLoop {
                provider,
                tool_registry,
                workspace_root: workspace_root_clone,
                descriptor: descriptor_clone,
                prompt_context: prompt_context_arc,
                cancel: cancel_for_task,
                sink: StreamSink { tx: Arc::new(tx) },
            };
            loop_.drive(input).await;
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }
}

impl RegistryItem for ReActAgent {
    fn name(&self) -> &str {
        &self.descriptor.name
    }

    fn description(&self) -> &str {
        &self.descriptor.description
    }
}

// ---------------------------------------------------------------------------
// ReActLoop — one session's driver (object-oriented core of the file)
// ---------------------------------------------------------------------------

/// Drives one ReAct session. Bound to the [`ReActAgent`]'s shared deps
/// plus per-call state (sender, cancellation). Encapsulates the full
/// state machine so the [`Agent::run`] implementation stays trivial.
struct ReActLoop {
    provider: Arc<dyn ModelProvider>,
    tool_registry: Arc<ToolRegistry>,
    workspace_root: PathBuf,
    /// Descriptor used by the prompt assembler to render the
    /// identity + adversarial-protocol sections. The base
    /// instructions live on `descriptor.instructions`.
    descriptor: AgentDescriptor,
    /// Manifest context (tools, skills, peer agents) the
    /// assembler injects into the system prompt. Shared as an
    /// `Arc` with [`ReActAgent`] so per-dispatch spawning never
    /// has to deep-clone the skills / peer-agents lists — the
    /// manifest is treated as read-only after the agent is
    /// constructed.
    prompt_context: Arc<PromptContext>,
    cancel: Arc<CancellationToken>,
    sink: StreamSink,
}

/// Result of one LLM sampling pass.
struct SampleOutcome {
    /// Plain text accumulated from the assistant (may be empty).
    assistant_text: String,
    /// Subset of the assistant turn that are tool calls.
    tool_uses: Vec<ToolUse>,
}

impl SampleOutcome {
    /// True if the model emitted at least one tool call.
    fn has_tool_calls(&self) -> bool {
        !self.tool_uses.is_empty()
    }
}

impl ReActLoop {
    /// Drive one session end-to-end. Emits every [`AgentEvent`] through
    /// the configured [`StreamSink`].
    ///
    /// Logging shape: `info!` on session start / per-iteration boundary /
    /// session end, `debug!` on tool-batch boundaries, `warn!` on the
    /// MAX_ITERATIONS guard. The outer span carries
    /// `agent`, `max_iterations`, and the iteration counter; turn on
    /// `RUST_LOG=synthia_agent::agent::re_act=debug` to inspect the
    /// full execution flow (assistant text length, tool-call counts,
    /// parallel vs sequential bucket sizes, per-tool completion).
    #[instrument(
        name = "react_loop",
        level = "info",
        skip_all,
        fields(
            agent = %self.descriptor.name,
            max_iterations = MAX_ITERATIONS,
        ),
    )]
    async fn drive(self, input: AgentInput) -> AgentOutput {
        info!(
            history_len = input.history.len(),
            "session start: preparing messages and dispatching LLM loop"
        );
        self.sink.system(SystemEvent::SessionStarted {
            session_id: String::new(),
        });

        let mut messages = self.prepare(&input);
        let mut end_reason = SessionEndReason::Completed;
        // `false` after any `break`; still `true` if the for-loop
        // exhausts naturally (i.e. we hit MAX_ITERATIONS).
        let mut exhausted = true;

        'outer: for iteration in 0..MAX_ITERATIONS {
            // Step boundary check: cancellation between iterations.
            if self.cancelled("cancelled before LLM call") {
                info!(
                    iteration,
                    "cancelled before LLM call; ending session with Cancelled"
                );
                end_reason = SessionEndReason::Cancelled;
                exhausted = false;
                break;
            }

            self.sink.system(SystemEvent::Progress {
                message: format!("LLM pass {iteration}"),
                step: iteration,
                total: MAX_ITERATIONS,
            });

            // Step 2: sample.
            let outcome = match self.sample_once(&messages, iteration).await {
                Ok(out) => out,
                Err(reason) => {
                    info!(
                        iteration,
                        ?reason,
                        "sample_once returned Err; ending session"
                    );
                    end_reason = reason;
                    exhausted = false;
                    break 'outer;
                }
            };

            // Step 3: commit assistant turn to history.
            self.commit_assistant(&mut messages, &outcome);

            if !outcome.has_tool_calls() {
                info!(
                    iteration,
                    assistant_text_len = outcome.assistant_text.len(),
                    "no tool calls returned by model; ending session with Completed"
                );
                end_reason = SessionEndReason::Completed;
                exhausted = false;
                break;
            }

            // Step 4: execute every tool call. Tools reporting
            // `ExecutionMode::Parallel` run concurrently via
            // `join_all`; `ExecutionMode::Sequential` tools run
            // one-at-a-time and abort the round on error.
            // Either way, the messages + events are emitted in
            // the order the LLM returned the tool calls so the
            // conversation history stays stable.
            if !outcome.tool_uses.is_empty()
                && self.cancelled("cancelled before tool execution")
            {
                info!(
                    iteration,
                    tool_call_count = outcome.tool_uses.len(),
                    "cancelled before tool execution; ending session with Cancelled"
                );
                end_reason = SessionEndReason::Cancelled;
                return self.finalize(end_reason, &messages);
            }
            info!(
                iteration,
                tool_call_count = outcome.tool_uses.len(),
                "step 4: executing tool calls returned by LLM"
            );
            self.execute_tools(&mut messages, &outcome.tool_uses).await;
        }

        if exhausted {
            warn!("hit MAX_ITERATIONS ({MAX_ITERATIONS})");
            self.sink.system(SystemEvent::Warning {
                kind: WarningKind::Loop,
                message: format!("hit MAX_ITERATIONS ({MAX_ITERATIONS})"),
                iteration: None,
            });
            end_reason = SessionEndReason::MaxIterations;
        }

        info!(
            end_reason = ?end_reason,
            history_len = messages.len(),
            "session end: finalize()"
        );
        self.finalize(end_reason, &messages)
    }

    // -- Step 1: prepare --------------------------------------------------

    /// Seed the message history with the assembled system prompt,
    /// any pre-existing `input.history` entries, and the new user
    /// prompt.
    ///
    /// The system prompt is built by
    /// [`crate::prompt::PromptContext::assemble`], which renders
    /// five delimited sections:
    ///
    /// 1. Identity (persona + role).
    /// 2. Tool manifest (every registered tool, with description).
    /// 3. Skill manifest (every enabled skill — disabled ones are
    ///    hidden so the LLM does not try to invoke them).
    /// 4. Peer-agent manifest (every other registered agent,
    ///    suitable for handoff).
    /// 5. Operating rules (output discipline + manifest grounding).
    ///
    /// The base instructions configured on the agent
    /// (`descriptor.instructions`) are prepended verbatim so
    /// callers retain full control of persona / tone.
    fn prepare(&self, input: &AgentInput) -> Vec<Message> {
        debug!(
            agent = %self.descriptor.name,
            history_len = input.history.len(),
            has_per_dispatch_ctx = input.prompt_context.is_some(),
            "step 1 prepare: assembling system prompt + history"
        );
        // Per-dispatch manifest (set via `AgentInput::with_prompt_context`)
        // wins over the agent's snapshot manifest so the server-side
        // dispatcher can rebuild it from the live registries on every
        // request without mutating the shared `Arc<dyn Agent>`.
        //
        // We hand the `Arc<PromptContext>` straight to the assembler
        // — no per-dispatch deep clone. The previous code cloned
        // `self.prompt_context` (a full `Vec<(String, String)>` +
        // `Vec<AgentDescriptor>` deep copy) on every chat turn;
        // that's tens of KB of allocations per dispatch for a
        // manifest that never mutates after boot.
        let arc_fallback: Arc<PromptContext>;
        let ctx: &PromptContext = match input.prompt_context.as_ref() {
            Some(c) => c,
            None => {
                arc_fallback = Arc::clone(&self.prompt_context);
                &arc_fallback
            }
        };
        // Per-dispatch runtime facts (cwd, worktree, platform,
        // date, model id) feed the `<env>` block. Built fresh
        // every call so the model's view tracks the loop's
        // actual execution context. `is_git_repo` is left
        // `None` — synthia-agent has no git dep; callers that
        // want a definitive value can override via
        // `Environment { is_git_repo: Some(...), .. }` on a
        // custom `ReActAgent` subclass.
        let env =
            Environment::from_runtime(&self.workspace_root.to_string_lossy());
        let assembled =
            ctx.clone().with_environment(env).assemble(&self.descriptor);
        debug!(
            agent = %self.descriptor.name,
            assembled_len = assembled.len(),
            skills = ctx.skills.len(),
            peer_agents = ctx.agents.len(),
            "step 1 prepare: system prompt assembled"
        );
        let mut messages: Vec<Message> =
            Vec::with_capacity(input.history.len() + 2);
        if !assembled.is_empty() {
            messages.push(Message::system(assembled));
        }
        messages.extend(input.history.iter().cloned());
        messages.push(input.to_message());
        debug!(
            agent = %self.descriptor.name,
            final_messages_len = messages.len(),
            "step 1 prepare: messages vector ready for LLM"
        );
        messages
    }

    // -- Step 2: sample ---------------------------------------------------

    /// Run one LLM sampling pass and translate every chunk to events.
    ///
    /// Returns `Err(SessionEndReason)` for cancellation or fatal stream
    /// errors. A successful return contains the assembled assistant
    /// text + parts + tool uses.
    async fn sample_once(
        &self,
        messages: &[Message],
        iteration: usize,
    ) -> Result<SampleOutcome, SessionEndReason> {
        let tools = self.tool_definitions().await;

        // Prefer the descriptor's `model_hint` so per-agent
        // model selection (e.g. a Judge routed through a
        // stronger reasoning model) reaches the provider.
        // `None` falls back to whatever default the provider
        // already configures internally.
        let model = self.descriptor.model_hint.clone().unwrap_or_default();
        info!(
            iteration,
            model = %model,
            messages_len = messages.len(),
            tool_definitions_len = tools.len(),
            "step 2 sample_once: dispatching streaming completion request to provider"
        );

        let req = CompletionRequest {
            model,
            messages: Arc::new(messages.to_vec()),
            tools: Arc::new(tools),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };

        let cancel = (*self.cancel).clone();
        let cb_cancel = cancel.clone();
        let state = ChunkState::default();
        let cb_state = state.clone();
        let cb_sink = self.sink.clone();
        let result = self
            .provider
            .complete_with_stream(
                req,
                Some(cancel.clone()),
                Box::new(move |chunk| {
                    if cb_cancel.is_cancelled() {
                        return;
                    }
                    cb_sink.ingest_chunk(&cb_state, chunk);
                }),
            )
            .await;

        if cancel.is_cancelled() {
            info!(iteration, "step 2 sample_once: cancelled during LLM stream");
            self.sink.system(SystemEvent::SessionInterrupted {
                reason: "cancelled during LLM stream".to_string(),
            });
            return Err(SessionEndReason::Cancelled);
        }

        match result {
            Ok(resp) => {
                let outcome = self.sink.finalize_outcome(&state);
                info!(
                    iteration,
                    response_id = %resp.id,
                    response_model = %resp.model,
                    prompt_tokens = resp.usage.prompt_tokens,
                    completion_tokens = resp.usage.completion_tokens,
                    cached = resp.cached,
                    stop_reason = ?resp.stop_reason,
                    assistant_text_len = outcome.assistant_text.len(),
                    tool_call_count = outcome.tool_uses.len(),
                    "step 2 sample_once: streaming completion returned"
                );
                Ok(outcome)
            }
            Err(e) => {
                warn!(
                    iteration,
                    error = %e,
                    "step 2 sample_once: provider returned error"
                );
                self.sink.system(SystemEvent::Warning {
                    kind: WarningKind::Hook,
                    message: format!("LLM stream error: {e}"),
                    iteration: Some(iteration),
                });
                Err(SessionEndReason::Error(e.to_string()))
            }
        }
    }

    async fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_registry
            .list(None)
            .await
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|e| {
                        let tool = e.tool_instance();
                        ToolDefinition {
                            name: tool.name().to_string(),
                            description: tool.description().to_string(),
                            input_schema: tool.parameters(),
                            cache_control: None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    // -- Step 3: commit ---------------------------------------------------

    /// Append the assembled assistant turn (text + tool uses) to history.
    fn commit_assistant(
        &self,
        messages: &mut Vec<Message>,
        outcome: &SampleOutcome,
    ) {
        if outcome.assistant_text.is_empty() && outcome.tool_uses.is_empty() {
            return;
        }

        let mut history_parts: Vec<ContentPart> = Vec::new();
        if !outcome.assistant_text.is_empty() {
            history_parts.push(ContentPart::Text(TextContent {
                text: outcome.assistant_text.clone(),
                cache_control: None,
            }));
        }
        for call in &outcome.tool_uses {
            history_parts.push(ContentPart::ToolUse(call.clone()));
        }

        messages.push(Message {
            role: Role::Assistant,
            content: Content::parts(history_parts),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        });
    }

    // -- Step 4: execute --------------------------------------------------

    /// Execute every tool call returned in one LLM pass.
    ///
    /// Tools advertising [`synthia_tool::traits::ExecutionMode::Parallel`]
    /// (the default for read-only / idempotent tools) run concurrently
    /// via [`futures::future::join_all`]. Tools advertising
    /// [`synthia_tool::traits::ExecutionMode::Sequential`] run strictly
    /// one-at-a-time and abort the round on the first error. Tools the
    /// registry cannot resolve are treated as Sequential (safe default).
    ///
    /// ToolProgress events fire as soon as a streaming tool emits them;
    /// the corresponding ToolResult (and the `messages` entry) is
    /// committed in the LLM's original tool-call order so the next
    /// sampling pass sees the history in the order it expects.
    async fn execute_tools(
        &self,
        messages: &mut Vec<Message>,
        calls: &[ToolUse],
    ) {
        if calls.is_empty() {
            return;
        }
        debug!(
            tool_call_count = calls.len(),
            "step 4 execute_tools: bucketing by ExecutionMode (Parallel vs Sequential)"
        );

        // 1. Bucket by ExecutionMode. Resolution failures default to
        //    Sequential so a missing tool never races with siblings.
        let mut safe_indices: Vec<usize> = Vec::new();
        let mut unsafe_indices: Vec<usize> = Vec::new();
        for (idx, call) in calls.iter().enumerate() {
            let mode = self.lookup_mode(&call.name).await;
            if mode == synthia_tool::traits::ExecutionMode::Parallel {
                safe_indices.push(idx);
            } else {
                unsafe_indices.push(idx);
            }
        }
        info!(
            tool_call_count = calls.len(),
            parallel_count = safe_indices.len(),
            sequential_count = unsafe_indices.len(),
            "step 4 execute_tools: bucketed; running Parallel via join_all, Sequential serially"
        );

        // 2. Run the Parallel bucket concurrently. Collect outputs in
        //    original order so commit_tool_result preserves the LLM's
        //    tool-call order.
        let mut outputs: Vec<Option<synthia_tool::ToolOutput>> =
            (0..calls.len()).map(|_| None).collect();
        if !safe_indices.is_empty() {
            let futs = safe_indices.iter().map(|&idx| {
                let call = &calls[idx];
                async move { (idx, self.execute_tool_inner(call).await) }
            });
            for (idx, out) in futures::future::join_all(futs).await {
                outputs[idx] = Some(out);
            }
        }

        // 3. Run the Sequential bucket strictly in order.
        let total_sequential = unsafe_indices.len();
        let mut sequential_aborted = false;
        for (seq_pos, idx) in unsafe_indices.into_iter().enumerate() {
            if self.cancel.is_cancelled() {
                debug!(
                    tool_call_count = calls.len(),
                    remaining_sequential = total_sequential - seq_pos,
                    "step 4 execute_tools: cancelled mid-Sequential batch"
                );
                break;
            }
            let call = &calls[idx];
            self.sink.system(SystemEvent::Progress {
                message: format!("Executing tool {}", call.name),
                step: 0,
                total: MAX_ITERATIONS,
            });
            let out = self.execute_tool_inner(call).await;
            let abort = out.is_error.unwrap_or(false);
            outputs[idx] = Some(out);
            if abort {
                warn!(
                    tool_name = %call.name,
                    tool_use_id = %call.id,
                    "step 4 execute_tools: sequential tool reported error; aborting remaining sequential calls"
                );
                sequential_aborted = true;
                break;
            }
        }
        if sequential_aborted {
            debug!(
                tool_call_count = calls.len(),
                "step 4 execute_tools: Sequential round aborted after first error"
            );
        }

        // 4. Commit ToolResults in LLM-call order. Missing outputs
        //    (cancellation, prior Sequential-tool error) become
        //    a synthetic error so the LLM still sees a result
        //    for every tool_use_id it requested. The synthetic
        //    message is intentionally neutral — it tells the
        //    model "this tool did not run" without leaking
        //    internal failure details (e.g. an earlier tool's
        //    secret) into the conversation history.
        let mut synthetic_count = 0usize;
        let mut error_count = 0usize;
        for (idx, call) in calls.iter().enumerate() {
            let output = outputs[idx].clone().unwrap_or_else(|| {
                synthia_tool::ToolOutput::error("tool did not produce a result")
            });
            let is_error = output.is_error.unwrap_or(false);
            if outputs[idx].is_none() {
                synthetic_count += 1;
            }
            if is_error {
                error_count += 1;
            }
            // Convert the tool-crate `TruncatedBy` enum into
            // its `serde_json::Value` form so the wire
            // `ToolResult` (in synthia_provider) can carry
            // it without pulling in a synthia_tool
            // dependency.
            let truncated_by = output
                .truncated_by
                .as_ref()
                .and_then(|t| serde_json::to_value(t).ok());
            self.commit_tool_result(
                messages,
                WireToolResult {
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    content: output.content,
                    is_error,
                    metadata: output.metadata,
                    truncated_by,
                },
            );
        }
        info!(
            tool_call_count = calls.len(),
            error_count,
            synthetic_count,
            history_len = messages.len(),
            "step 4 execute_tools: all ToolResults committed to history"
        );
    }

    /// Resolve a tool's [`ExecutionMode`] from the registry.
    ///
    /// Returns [`ExecutionMode::Sequential`] for any tool the registry
    /// cannot resolve — the conservative default that preserves the
    /// pre-parallel behaviour for missing tools.
    async fn lookup_mode(
        &self,
        name: &str,
    ) -> synthia_tool::traits::ExecutionMode {
        match self.tool_registry.get(name).await {
            Ok(Some(entry)) => entry.tool_instance().mode(),
            _ => synthia_tool::traits::ExecutionMode::Sequential,
        }
    }

    /// Run a single tool call to completion and emit any
    /// `ToolProgress` events. Does NOT commit to `messages` —
    /// the caller (`execute_tools`) decides commit order so the
    /// history stays aligned with the LLM's tool-call order.
    async fn execute_tool_inner(
        &self,
        call: &ToolUse,
    ) -> synthia_tool::ToolOutput {
        let tool_name = call.name.clone();
        let call_id = call.id.clone();
        debug!(
            tool_name = %tool_name,
            tool_use_id = %call_id,
            "execute_tool_inner: invoking tool"
        );

        let entry = match self.tool_registry.get(&tool_name).await {
            Ok(Some(e)) => e,
            Ok(None) => {
                warn!(
                    tool_name = %tool_name,
                    tool_use_id = %call_id,
                    "execute_tool_inner: tool not found in registry"
                );
                return synthia_tool::ToolOutput::error(format!(
                    "tool not found: {tool_name}"
                ));
            }
            Err(e) => {
                warn!(
                    tool_name = %tool_name,
                    tool_use_id = %call_id,
                    error = %e,
                    "execute_tool_inner: registry lookup error"
                );
                return synthia_tool::ToolOutput::error(format!(
                    "registry error: {e}"
                ));
            }
        };

        let ctx = Context::new(String::new(), self.workspace_root.clone());
        let tool = entry.tool_instance();
        let mut stream = tool.stream(call.input.clone(), &ctx);
        let mut final_output: Option<synthia_tool::ToolOutput> = None;

        loop {
            if self.cancel.is_cancelled() {
                debug!(
                    tool_name = %tool_name,
                    tool_use_id = %call_id,
                    "execute_tool_inner: cancelled mid-stream"
                );
                break;
            }
            match stream.next().await {
                Some(StreamOutput::Progress(output)) => {
                    self.sink.system(SystemEvent::ToolProgress {
                        tool_name: tool_name.clone(),
                        call_id: call_id.clone(),
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
            synthia_tool::ToolOutput::error(
                "tool stream closed without producing a Result",
            )
        });
        debug!(
            tool_name = %tool_name,
            tool_use_id = %call_id,
            is_error = result.is_error.unwrap_or(false),
            "execute_tool_inner: tool stream completed"
        );
        result
    }

    fn commit_tool_result(
        &self,
        messages: &mut Vec<Message>,
        tr: WireToolResult,
    ) {
        messages.push(Message::tool(
            Content::parts(tr.content.clone()),
            tr.call_id.clone(),
        ));
        // Forward the tool-output metadata and truncated-by
        // markers onto the wire `ToolResult` so downstream
        // consumers (A2A, frontend, persisted history) can
        // see the tool's telemetry. The fields are
        // `#[serde(skip_serializing_if = …)]`-guarded so an
        // empty map / None never inflates the wire payload
        // for the common case where the tool set neither.
        self.sink.model(ContentPart::ToolResult(ToolResult {
            tool_use_id: tr.call_id,
            tool_name: Some(tr.tool_name),
            content: tr.content,
            structured_content: None,
            is_error: Some(tr.is_error),
            metadata: tr.metadata,
            truncated_by: tr.truncated_by,
        }));
    }

    // -- Step 5: finalize -------------------------------------------------

    /// Emit the terminal event and build the [`AgentOutput`].
    fn finalize(
        &self,
        reason: SessionEndReason,
        messages: &[Message],
    ) -> AgentOutput {
        let final_message = last_assistant_text(messages);
        self.sink.system(SystemEvent::SessionEnded { reason });
        AgentOutput {
            events: Vec::new(),
            final_message,
        }
    }

    // -- helpers ----------------------------------------------------------

    /// Emit a `SessionInterrupted` event and return whether the token is
    /// cancelled. Centralizes the "check + emit" pair.
    fn cancelled(&self, reason: &str) -> bool {
        if !self.cancel.is_cancelled() {
            return false;
        }
        self.sink.system(SystemEvent::SessionInterrupted {
            reason: reason.to_string(),
        });
        true
    }
}

// ---------------------------------------------------------------------------
// StreamSink — typed wrapper over the mpsc sender
// ---------------------------------------------------------------------------

/// Per-tool-use accumulator while a streaming provider emits
/// `ToolCallStart { id, name, arguments }` → `ToolCallDelta` →
/// `ToolCallEnd { id }`. We accumulate the raw JSON string and parse it
/// once on `ToolCallEnd` (or session end as a fallback).
#[derive(Debug, Clone)]
struct ToolUseBuffer {
    id: String,
    name: String,
    arguments: String,
}

/// Mutable state owned by [`StreamSink`] during a single LLM sampling
/// pass. Wrapped in `Arc<Mutex<>>` because the streaming provider's
/// callback (`FnMut + Send + 'static`) outlives the borrow scope of
/// `sample_once`.
#[derive(Debug, Default, Clone)]
struct ChunkState(Arc<std::sync::Mutex<ChunkStateInner>>);

#[derive(Debug, Default)]
struct ChunkStateInner {
    tool_buffers: HashMap<String, ToolUseBuffer>,
    assistant_text: String,
    assistant_parts: Vec<ContentPart>,
    /// Whether at least one streamed `Content(Text)` chunk
    /// has arrived. Used by the `IsDone` branch to decide
    /// whether to re-emit `result.text` (fallback for
    /// non-streaming providers that batch everything into
    /// `IsDone`).
    saw_streamed_text: bool,
    /// `tool_use_id`s already pushed to `assistant_parts`
    /// from streamed `ToolCallEnd` chunks. The IsDone
    /// `result.tool_calls` re-listing is deduped against this
    /// set so we never push the same tool call twice into
    /// the history builder.
    seen_tool_ids: HashSet<String>,
}

/// Typed wrapper over the mpsc sender. Owns an `Arc` to the sender
/// so it can be cloned into `'static` streaming callbacks.
#[derive(Clone)]
struct StreamSink {
    tx: Arc<mpsc::UnboundedSender<AgentEvent>>,
}

impl StreamSink {
    /// Translate one streaming chunk into events + state mutation.
    fn ingest_chunk(&self, state: &ChunkState, chunk: StreamChunk) {
        let mut guard = state.0.lock().expect("chunk state poisoned");
        handle_chunk(chunk, &mut guard, &self.tx);
    }

    /// Drain any remaining tool buffers on early stream termination.
    /// Returns the assembled outcome for the caller.
    fn finalize_outcome(&self, state: &ChunkState) -> SampleOutcome {
        let mut guard = state.0.lock().expect("chunk state poisoned");
        let tool_buffers = std::mem::take(&mut guard.tool_buffers);
        let mut assistant_parts = std::mem::take(&mut guard.assistant_parts);
        // Pass `seen_tool_ids` so `finalize_buffers` can skip
        // any partial buffer entries whose tool_use_id was
        // already fully drained via `ToolCallEnd` or surfaced
        // from `IsDone.result.tool_calls`. Without this, a
        // stream that ends mid-ToolCall (no `ToolCallEnd`)
        // but then lists the same call in `IsDone` would push
        // the partial buffer's ToolUse AND the IsDone's
        // ToolUse into `assistant_parts`, producing a
        // duplicate in the history.
        let seen_tool_ids = std::mem::take(&mut guard.seen_tool_ids);
        finalize_buffers(
            tool_buffers,
            seen_tool_ids,
            &mut assistant_parts,
            &self.tx,
        );
        SampleOutcome {
            assistant_text: std::mem::take(&mut guard.assistant_text),
            tool_uses: extract_tool_uses(&assistant_parts),
        }
    }

    // -- typed emit helpers ----------------------------------------------

    fn emit(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    fn system(&self, event: SystemEvent) {
        self.emit(AgentEvent::System(event));
    }

    fn model(&self, part: ContentPart) {
        self.emit(AgentEvent::Model(part));
    }
}

// ---------------------------------------------------------------------------
// Free helpers — kept because they operate on plain data, no state.
// ---------------------------------------------------------------------------

/// Translate one chunk; mutates `state`; emits events through `tx`.
fn handle_chunk(
    chunk: StreamChunk,
    state: &mut ChunkStateInner,
    tx: &Arc<mpsc::UnboundedSender<AgentEvent>>,
) {
    match chunk {
        StreamChunk::Content(part) => {
            if let ContentPart::Text(t) = &part {
                state.assistant_text.push_str(&t.text);
                state.saw_streamed_text = true;
            }
            state.assistant_parts.push(part.clone());
            let _ = tx.send(AgentEvent::Model(part));
        }
        StreamChunk::Usage(_) => {
            // Intermediate usage chunks are intentionally NOT
            // surfaced as `AgentEvent::System(SystemEvent::Usage)`
            // events: providers such as Anthropic emit
            // `message_delta.usage` *before* the terminal
            // `message.usage`, and `message.usage` is the
            // authoritative count for the iteration. The final
            // `Usage` event is emitted from the `IsDone` branch
            // below, so consumers see exactly one Usage per
            // LLM iteration instead of two.
        }
        StreamChunk::Stop(_) => {
            // Stop reasons are recovered downstream from the final
            // `SamplingResult`; nothing to emit here.
        }
        StreamChunk::ToolCallStart {
            id,
            name,
            arguments,
        } => {
            let initial_args = match arguments {
                Value::String(s) => s,
                other => other.to_string(),
            };
            state.tool_buffers.insert(
                id.clone(),
                ToolUseBuffer {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: initial_args,
                },
            );
        }
        StreamChunk::ToolCallDelta {
            id,
            arguments_delta,
        } => {
            if let Some(buf) = state.tool_buffers.get_mut(&id) {
                buf.arguments.push_str(&arguments_delta);
            }
        }
        StreamChunk::ToolCallEnd { id } => {
            if let Some(buf) = state.tool_buffers.remove(&id) {
                let parsed = parse_tool_input(&buf.arguments);
                let tool_use = ToolUse {
                    id: buf.id.clone(),
                    name: buf.name.clone(),
                    input: parsed,
                };
                state
                    .assistant_parts
                    .push(ContentPart::ToolUse(tool_use.clone()));
                state.seen_tool_ids.insert(tool_use.id.clone());
                let _ =
                    tx.send(AgentEvent::Model(ContentPart::ToolUse(tool_use)));
            }
        }
        StreamChunk::IsDone { result } => {
            let result = *result;
            // Only emit `result.text` if no streamed text
            // chunk arrived — this avoids double-emitting the
            // same text twice when streaming providers include
            // the consolidated final text in `IsDone`. Same
            // reasoning for `result.tool_calls` vs streamed
            // `ToolCallEnd` chunks: the IsDone re-listing is
            // deduped against `seen_tool_ids` so we never push
            // the same tool_use_id into `assistant_parts`
            // twice (which would put duplicate tool calls into
            // the history message).
            //
            // `state.assistant_text` is intentionally NOT
            // mutated by `result.text` here when streamed
            // chunks preceded IsDone: `assistant_text` already
            // holds the full string and folding `result.text`
            // in again would duplicate it.
            if !state.saw_streamed_text && !result.text.is_empty() {
                state.assistant_text.push_str(&result.text);
                let text_part = ContentPart::Text(TextContent {
                    text: result.text.clone(),
                    cache_control: None,
                });
                state.assistant_parts.push(text_part.clone());
                let _ = tx.send(AgentEvent::Model(text_part));
            }
            for call in &result.tool_calls {
                if state.seen_tool_ids.contains(&call.id) {
                    continue;
                }
                state
                    .assistant_parts
                    .push(ContentPart::ToolUse(call.clone()));
                let _ = tx.send(AgentEvent::Model(ContentPart::ToolUse(
                    call.clone(),
                )));
                state.seen_tool_ids.insert(call.id.clone());
            }
            let usage = result.usage.clone();
            let _ = tx.send(AgentEvent::usage(
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.cache_read_tokens,
                usage.cache_write_tokens,
            ));
            let _ = tx.send(AgentEvent::ModelDone(result));
        }
    }
}

/// Drain remaining `ToolUseBuffer`s on early stream termination
/// (network cut-off, missing `ToolCallEnd`). Best-effort.
///
/// `seen_tool_ids` is consulted before pushing each leftover
/// ToolUse into `assistant_parts` / wire. If the id was
/// already drained (via a `ToolCallEnd` chunk) or surfaced
/// (via `IsDone.result.tool_calls`), the partial buffer is
/// silently dropped to avoid double-emitting the same id
/// into the history.
fn finalize_buffers(
    tool_buffers: HashMap<String, ToolUseBuffer>,
    seen_tool_ids: HashSet<String>,
    assistant_parts: &mut Vec<ContentPart>,
    tx: &Arc<mpsc::UnboundedSender<AgentEvent>>,
) {
    if tool_buffers.is_empty() {
        return;
    }
    for (_, buf) in tool_buffers {
        if seen_tool_ids.contains(&buf.id) {
            // Already drained elsewhere — skip the partial
            // duplicate.
            continue;
        }
        let parsed = parse_tool_input(&buf.arguments);
        let tool_use = ToolUse {
            id: buf.id.clone(),
            name: buf.name.clone(),
            input: parsed,
        };
        assistant_parts.push(ContentPart::ToolUse(tool_use.clone()));
        let _ = tx.send(AgentEvent::Model(ContentPart::ToolUse(tool_use)));
    }
}

#[cfg(test)]
mod finalize_buffers_tests {
    //! Direct unit tests for [`super::finalize_buffers`].
    //!
    //! The streaming integration path drains tool buffers via
    //! `finalize_outcome` after every successful sample_once
    //! call, but the production wiring is exercised through
    //! a multi-chunk scripted provider. These tests target the
    //! helper directly so the dedup contract is locked
    //! independently of integration assumptions.

    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::mpsc;

    use super::{ToolUseBuffer as AgentToolUseBuffer, *};

    fn make_buf(id: &str, args: &str) -> AgentToolUseBuffer {
        AgentToolUseBuffer {
            id: id.to_string(),
            name: "test_tool".to_string(),
            arguments: args.to_string(),
        }
    }

    fn make_sink() -> (
        Arc<mpsc::UnboundedSender<AgentEvent>>,
        mpsc::UnboundedReceiver<AgentEvent>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Arc::new(tx), rx)
    }

    #[test]
    fn skips_buffer_whose_id_already_drained_via_seen_tool_ids() {
        // A stream that ends mid-ToolCall (no `ToolCallEnd`)
        // but lists the call in `IsDone.result.tool_calls`
        // would otherwise push BOTH the partial buffer AND
        // the IsDone tool_use into `assistant_parts`. This
        // test pins the dedup: `seen_tool_ids` blocks the
        // partial buffer from re-emitting.
        let mut buffers: HashMap<String, AgentToolUseBuffer> = HashMap::new();
        buffers.insert("partial".to_string(), make_buf("partial", "{\"x\":1"));
        // Other buffer that DID complete normally via
        // `ToolCallEnd` and was already recorded in
        // `seen_tool_ids`.
        buffers
            .insert("completed".to_string(), make_buf("completed", "{\"y\":2"));
        let mut seen = HashSet::new();
        seen.insert("completed".to_string());
        let mut parts: Vec<ContentPart> = Vec::new();
        let (tx, mut rx) = make_sink();

        finalize_buffers(buffers, seen, &mut parts, &tx);

        // The buffer named "partial" should have been pushed
        // (it had no ToolCallEnd, so seen_tool_ids didn't
        // contain it). The "completed" buffer should have
        // been dropped.
        let ids: Vec<&str> = parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::ToolUse(tu) => Some(tu.id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec!["partial"],
            "seen_tool_ids must suppress duplicate drain; got {ids:?}"
        );

        // Wire side: only "partial" was sent.
        let mut wire_ids: Vec<String> = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let AgentEvent::Model(ContentPart::ToolUse(tu)) = ev {
                wire_ids.push(tu.id);
            }
        }
        assert_eq!(wire_ids, vec!["partial".to_string()]);
    }

    #[test]
    fn keeps_buffer_for_id_not_in_seen_set() {
        // Symmetric case: when a partial buffer's id is NOT
        // in `seen_tool_ids`, it must surface — otherwise a
        // partial tool call would silently disappear.
        let mut buffers: HashMap<String, AgentToolUseBuffer> = HashMap::new();
        buffers.insert("partial".to_string(), make_buf("partial", "{}"));
        let seen: HashSet<String> = HashSet::new();
        let mut parts: Vec<ContentPart> = Vec::new();
        let (tx, _rx) = make_sink();

        finalize_buffers(buffers, seen, &mut parts, &tx);

        assert_eq!(parts.len(), 1);
        match &parts[0] {
            ContentPart::ToolUse(tu) => {
                assert_eq!(tu.id, "partial");
                // The parsed input should be a JSON object
                // (default-Value for `"{}"` parse).
                assert_eq!(tu.input, json!({}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn empty_buffer_early_returns() {
        // Quick no-op check: empty buffers + empty seen
        // must not push anything.
        let buffers: HashMap<String, AgentToolUseBuffer> = HashMap::new();
        let seen: HashSet<String> = HashSet::new();
        let mut parts: Vec<ContentPart> = Vec::new();
        let (tx, mut rx) = make_sink();

        finalize_buffers(buffers, seen, &mut parts, &tx);

        assert!(parts.is_empty());
        assert!(rx.try_recv().is_err());
    }
}

fn extract_tool_uses(parts: &[ContentPart]) -> Vec<ToolUse> {
    parts
        .iter()
        .filter_map(|p| {
            if let ContentPart::ToolUse(tu) = p {
                Some(tu.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Parse a tool input JSON string. Empty/invalid JSON yields
/// `Value::Object({})` / the raw string respectively.
fn parse_tool_input(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str(trimmed)
        .unwrap_or_else(|_| Value::String(raw.to_string()))
}

fn last_assistant_text(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, Role::Assistant))
        .and_then(|m| m.content.extract_text())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use serde_json::json;
    use synthia_core::Error;
    use synthia_provider::{
        CompletionResponse,
        ContentPart,
        ProviderConfig,
        SamplingResult,
        StreamChunk,
        TokenUsage,
        traits::ModelProvider,
        types::ModelConfig,
    };
    use synthia_tool::{Tool, ToolOutput, ToolRegistry};
    use test_support::FakeTool;
    use tokio::sync::Mutex as TokioMutex;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::events::WarningKind;

    // -- ScriptedStreamProvider -------------------------------------------

    /// Test provider that emits a pre-scripted sequence of
    /// `StreamChunk`s per `complete_with_stream` call.
    #[derive(Debug)]
    struct ScriptedStreamProvider {
        scripted: Arc<TokioMutex<Vec<Vec<StreamChunk>>>>,
        call_count: Arc<AtomicUsize>,
    }

    impl ScriptedStreamProvider {
        fn new(scripted: Vec<Vec<StreamChunk>>) -> Self {
            Self {
                scripted: Arc::new(TokioMutex::new(scripted)),
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        async fn take_next(&self) -> Vec<StreamChunk> {
            let mut guard = self.scripted.lock().await;
            if guard.is_empty() {
                return vec![StreamChunk::IsDone {
                    result: Box::new(SamplingResult::default()),
                }];
            }
            guard.remove(0)
        }
    }

    #[async_trait]
    impl ModelProvider for ScriptedStreamProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "scripted-stream"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "fake".to_string(),
                provider: "scripted".to_string(),
                context_window: 128_000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: true,
            }
        }

        async fn complete(
            &self,
            _request: synthia_provider::CompletionRequest,
        ) -> Result<CompletionResponse, Error> {
            unreachable!("streaming path should not call complete()")
        }

        async fn complete_with_stream(
            &self,
            _request: synthia_provider::CompletionRequest,
            _cancel_token: Option<CancellationToken>,
            mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
        ) -> Result<CompletionResponse, Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let chunks = self.take_next().await;
            let mut final_sampling: Option<SamplingResult> = None;
            for chunk in chunks {
                if let StreamChunk::IsDone { result } = &chunk {
                    final_sampling = Some((**result).clone());
                }
                on_delta(chunk);
            }
            let sampling = final_sampling.unwrap_or_default();
            Ok(CompletionResponse {
                id: format!("resp-{}", self.call_count.load(Ordering::SeqCst)),
                model: "fake".to_string(),
                content: synthia_provider::Content::Single(ContentPart::Text(
                    TextContent {
                        text: sampling.text.clone(),
                        cache_control: None,
                    },
                )),
                usage: sampling.usage.clone(),
                cached: false,
                stop_reason: sampling.stop_reason.clone(),
            })
        }

        async fn embed(
            &self,
            _texts: Vec<String>,
        ) -> Result<Vec<Vec<f64>>, Error> {
            Ok(vec![])
        }
    }

    fn empty_response() -> Vec<StreamChunk> {
        vec![StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "hi there".to_string(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    cached_prompt_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                },
                ..Default::default()
            }),
        }]
    }

    fn tool_call_response(tool_calls: Vec<ToolUse>) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();
        for call in &tool_calls {
            chunks.push(StreamChunk::ToolCallStart {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: serde_json::Value::String(
                    serde_json::to_string(&call.input).unwrap_or_default(),
                ),
            });
            chunks.push(StreamChunk::ToolCallEnd {
                id: call.id.clone(),
            });
        }
        // Real providers re-list tool calls in `IsDone`; the loop
        // consumes them only once (via `ToolCallEnd`), so emit empty
        // here to avoid double-counting.
        chunks.push(StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: String::new(),
                tool_calls: Vec::new(),
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage::default(),
                ..Default::default()
            }),
        });
        chunks
    }

    /// Variant of [`tool_call_response`] that also emits a
    /// `StreamChunk::Usage` carrying a non-default
    /// [`TokenUsage`]. Used by tests that need to assert the
    /// per-iteration Usage emission pipeline.
    fn tool_call_response_with_usage(
        tool_calls: Vec<ToolUse>,
        usage: TokenUsage,
    ) -> Vec<StreamChunk> {
        let mut chunks = vec![StreamChunk::Usage(usage)];
        chunks.extend(tool_call_response(tool_calls));
        chunks
    }

    /// Run one [`ReActAgent`] session and drain every [`AgentEvent`]
    /// through the channel. Returns the captured event sequence.
    async fn run_and_collect(
        provider: Arc<dyn ModelProvider>,
        registry: Arc<ToolRegistry>,
        cancel: CancellationToken,
        input: AgentInput,
    ) -> Vec<AgentEvent> {
        let agent = ReActAgent::new(provider, registry);
        let mut stream = agent.run(input, Arc::new(cancel)).await;
        let mut out = Vec::new();
        while let Some(ev) = stream.next().await {
            out.push(ev);
        }
        out
    }

    // -- Agent-level tests (descriptor + smoke) ---------------------------

    #[tokio::test]
    async fn system_prompt_is_injected_as_first_message() {
        // Regression: the previous ReActLoop prepared `messages`
        // from `input.history + input.to_message()` and never
        // inserted a system prompt, so the LLM ran without a role.
        // Verify the prompt is now stored on the agent and that
        // the public `run` path consumes it.
        use synthia_provider::Role;

        let agent = ReActAgent::with_options(
            Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            "TEST-PROMPT".to_string(),
        );

        // Verify the system message shape used in `prepare()`.
        let sys = Message::system("hello");
        assert!(matches!(sys.role, Role::System));

        // Drive the loop with a stub provider so it terminates
        // quickly. We only assert the wiring is in place; the stub
        // does not emit user-visible text.
        let mut stream = agent
            .run(AgentInput::text("hi"), Arc::new(CancellationToken::new()))
            .await;
        let mut events = Vec::new();
        while let Some(ev) = stream.next().await {
            events.push(ev);
        }
        assert!(
            !events.is_empty(),
            "agent.run must yield at least the SessionStarted + SessionEnded events"
        );
    }

    #[tokio::test]
    async fn descriptor_is_stable() {
        let agent = ReActAgent::new(
            Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
            Arc::new(ToolRegistry::new()),
        );
        assert_eq!(agent.descriptor().name, "agent");
        assert_eq!(agent.descriptor().kind, "react");
        assert_eq!(agent.descriptor().version, "1.0.0");
        assert!(
            agent
                .descriptor()
                .capabilities
                .contains(&"tools".to_string())
        );
        // Industry-aligned fields default to sensible values.
        assert_eq!(
            agent.descriptor().instructions,
            crate::agent::re_act::DEFAULT_SYSTEM_PROMPT
        );
        assert!(agent.descriptor().handoff_hint.is_some());
        assert_eq!(agent.descriptor().owner.as_deref(), Some("synthia"));
        assert_eq!(agent.descriptor().domain.as_deref(), Some("coding"));
        // Adversarial-panel defaults are gone after the refactor.
        assert!(agent.descriptor().persona.is_some());
        // The human-readable label is "Synthia" — the
        // programmatic `name` ("agent") stays as the
        // routing slug.
        assert_eq!(agent.descriptor().display_name(), "Synthia");
        assert_eq!(agent.descriptor().display_name.as_deref(), Some("Synthia"));
    }

    #[tokio::test]
    async fn descriptor_surfaces_custom_system_prompt() {
        let agent = ReActAgent::with_options(
            Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            "CUSTOM".to_string(),
        );
        // `system_prompt` argument is now exposed via
        // `descriptor.instructions` so callers can introspect
        // the agent's role without holding a second handle.
        assert_eq!(agent.descriptor().instructions, "CUSTOM");
    }

    /// The default system prompt is more than a one-liner —
    /// it covers action safety, tool use, output style, and
    /// markdown formatting in balanced XML-delimited blocks
    /// that line up with the sections the prompt assembler
    /// appends (`<identity>`, `<env>`, `<available_skills>`,
    /// `<available_agents>`). Pin the structural contracts so
    /// a future copy edit cannot silently drop a section.
    #[test]
    fn default_system_prompt_covers_canonical_sections() {
        let p = crate::agent::re_act::DEFAULT_SYSTEM_PROMPT;

        // Identity line — Synthia is the agent's name.
        assert!(p.contains("You are Synthia"));

        // Every canonical XML section is present and balanced.
        for tag in [
            "action_safety",
            "tool_calling",
            "output_efficiency",
            "formatting",
        ] {
            let open = format!("<{tag}>");
            let close = format!("</{tag}>");
            assert!(
                p.contains(&open) && p.contains(&close),
                "missing `{open}` / `{close}` in DEFAULT_SYSTEM_PROMPT"
            );
        }

        // Behavioural anchors — distilled from the Grok
        // Build and OpenCode default agent prompts.
        assert!(p.contains("confirm with the user"));
        assert!(p.contains("NEVER use shell"));
        assert!(p.contains("CommonMark"));
        assert!(p.contains("nothing more, nothing less"));
    }

    /// Default persona + handoff_hint are short, on-brand
    /// one-liners (not the long-form `DEFAULT_SYSTEM_PROMPT`
    /// body). Pinning the shape keeps the `<identity>` block
    /// readable when the assembler renders the descriptor.
    #[tokio::test]
    async fn descriptor_defaults_are_on_brand() {
        let agent = ReActAgent::new(
            Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
            Arc::new(ToolRegistry::new()),
        );
        let d = agent.descriptor();

        assert!(d.description.starts_with("Default Synthia agent"));
        assert!(
            d.handoff_hint
                .as_deref()
                .is_some_and(|s| s.contains("specialist agent")),
            "default handoff_hint should explain when to delegate"
        );
        assert!(
            d.persona
                .as_deref()
                .is_some_and(|s| s.starts_with("You are Synthia")),
            "default persona should self-identify as Synthia"
        );
        assert!(
            d.persona.as_deref().unwrap_or_default().len() < 120,
            "persona is a one-liner; long-form guidance lives in \
             DEFAULT_SYSTEM_PROMPT"
        );
    }

    // -- Prompt assembly integration tests -------------------------------

    fn tools_with(name: &str, desc: &str) -> synthia_provider::ToolDefinition {
        synthia_provider::ToolDefinition {
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: json!({"type": "object"}),
            cache_control: None,
        }
    }

    #[tokio::test]
    async fn with_prompt_context_injects_skills_and_agents() {
        // End-to-end: skills + peer agents populated via
        // `with_prompt_context` must reach the LLM in the
        // assembled system prompt.
        let provider =
            Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: "ok".into(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage::default(),
                    ..Default::default()
                }),
            }]]));

        let ctx = crate::prompt::PromptContext::default()
            .with_skill("summarize", "Summarize text.")
            .with_agent(&peer_descriptor(
                "planner",
                "Plans the work.",
                Some("complex tasks"),
            ));

        let agent = ReActAgent::with_prompt_context(
            provider.clone(),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            "BASE".to_string(),
            Arc::new(ctx),
        );

        let mut stream = agent
            .run(
                crate::input::AgentInput::text("hi"),
                Arc::new(CancellationToken::new()),
            )
            .await;
        while let Some(_ev) = stream.next().await {}
        drop(stream);

        for _ in 0..50 {
            if provider.call_count.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let captured = provider.captured.lock().await;
        assert!(!captured.is_empty());
        let sys_text = match &captured[0][0].content {
            synthia_provider::Content::Single(ContentPart::Text(t)) => {
                t.text.clone()
            }
            _ => panic!("expected single text content"),
        };
        assert!(sys_text.contains("BASE"));
        // The `<identity>` opening line must use the
        // human-readable `display_name` ("Synthia"), not
        // the routing slug ("agent"), so the model
        // self-identifies with the persona the user sees
        // on the UI / A2A card.
        assert!(
            sys_text.contains("You are `Synthia`"),
            "identity line must use display_name; got:\n{sys_text}"
        );
        assert!(
            !sys_text.contains("You are `react`"),
            "identity line must not leak the routing slug; got:\n{sys_text}"
        );
        // `read_file` is a tool — it must NOT appear in the
        // prompt text (tools ride the completion request's
        // `tools` channel).
        assert!(!sys_text.contains("`read_file`"));
        assert!(sys_text.contains("<name>summarize</name>"));
        assert!(
            !sys_text.contains("<name>disabled-skill</name>"),
            "disabled skills must not be advertised"
        );
        assert!(sys_text.contains("`planner`"));
        assert!(sys_text.contains("(use when: complex tasks)"));
        assert!(sys_text.contains("<identity>"));
        assert!(sys_text.contains("</identity>"));
        assert!(sys_text.contains("<available_skills>"));
        assert!(sys_text.contains("</available_skills>"));
        assert!(sys_text.contains("<available_agents>"));
        assert!(sys_text.contains("</available_agents>"));
        // Tool schemas are NOT in the prompt — they ride the
        // request's `tools` channel and the runtime validates
        // every emitted name.
        assert!(
            !sys_text.contains("<available_tools>"),
            "tool schemas must not be assembled into the system prompt"
        );
        assert!(
            !sys_text.contains("Use only these tools:"),
            "tool grounding belongs on the runtime, not in the prompt"
        );
    }

    #[tokio::test]
    async fn descriptor_is_stable_returns_assembled_prompt_metadata() {
        // Sanity: the descriptor fields used by the assembler
        // are reachable via `prompt_context()` and `descriptor()`.
        let agent = ReActAgent::with_prompt_context(
            Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            "X".to_string(),
            Arc::new(crate::prompt::PromptContext::default()),
        );
        assert_eq!(agent.prompt_context().skills.len(), 0);
        assert_eq!(agent.descriptor().instructions, "X");
    }

    #[tokio::test]
    async fn set_prompt_context_swaps_manifest_at_runtime() {
        // The setter lets callers (e.g. settings changes) swap
        // the manifest without rebuilding the agent.
        let mut agent = ReActAgent::with_options(
            Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            "X".to_string(),
        );
        assert!(agent.prompt_context().skills.is_empty());
        agent.set_prompt_context(
            crate::prompt::PromptContext::default().with_skill("y", "y"),
        );
        assert_eq!(agent.prompt_context().skills.len(), 1);
    }

    #[tokio::test]
    async fn tool_schemas_never_appear_in_assembled_system_prompt() {
        // End-to-end contract: even when tools exist in the
        // runtime, the assembled system prompt must NOT carry
        // their descriptions. Tool schemas ride the completion
        // request's `tools` field; the runtime validates every
        // emitted name against the registry.
        let provider =
            Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: "ok".into(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage::default(),
                    ..Default::default()
                }),
            }]]));

        // `PromptContext` no longer carries tools — it only
        // carries skills / peer agents. The runtime will pull
        // tool definitions from `ToolRegistry` for the
        // completion request's `tools` channel.
        let ctx = crate::prompt::PromptContext::default();

        let agent = ReActAgent::with_prompt_context(
            provider.clone(),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            "BASE".to_string(),
            Arc::new(ctx),
        );

        let mut stream = agent
            .run(
                crate::input::AgentInput::text("hi"),
                Arc::new(CancellationToken::new()),
            )
            .await;
        while let Some(_ev) = stream.next().await {}
        drop(stream);

        for _ in 0..50 {
            if provider.call_count.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let captured = provider.captured.lock().await;
        let sys_text = match &captured[0][0].content {
            synthia_provider::Content::Single(ContentPart::Text(t)) => {
                t.text.clone()
            }
            _ => panic!("expected single text content"),
        };
        // No tool manifest, no tool grounding, no tool name
        // leakage in the prompt text.
        assert!(
            !sys_text.contains("<available_tools>"),
            "tool schemas must not be assembled into the system prompt"
        );
        assert!(
            !sys_text.contains("Use only these tools:"),
            "tool grounding belongs on the runtime, not in the prompt"
        );
    }

    // -- End-to-end runtime injection ---------------------------------

    /// Provider that records every `CompletionRequest` it receives.
    /// Used to assert what the ReAct loop actually sends to the
    /// LLM, not just what is stored on the agent.
    struct CapturingProvider {
        captured: Arc<TokioMutex<Vec<Arc<Vec<synthia_provider::Message>>>>>,
        scripted: Arc<TokioMutex<Vec<Vec<StreamChunk>>>>,
        call_count: AtomicUsize,
    }

    impl CapturingProvider {
        fn new(scripted: Vec<Vec<StreamChunk>>) -> Self {
            Self {
                captured: Arc::new(TokioMutex::new(Vec::new())),
                scripted: Arc::new(TokioMutex::new(scripted)),
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ModelProvider for CapturingProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "capturing"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "capturing".into(),
                provider: "test".into(),
                context_window: 128_000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: synthia_provider::types::CompletionRequest,
        ) -> Result<CompletionResponse, Error> {
            unreachable!("streaming path")
        }

        async fn complete_with_stream(
            &self,
            request: synthia_provider::types::CompletionRequest,
            _cancel: Option<tokio_util::sync::CancellationToken>,
            mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
        ) -> Result<CompletionResponse, Error> {
            self.captured.lock().await.push(request.messages.clone());
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let chunks = {
                let mut g = self.scripted.lock().await;
                if g.is_empty() {
                    vec![StreamChunk::IsDone {
                        result: Box::new(SamplingResult::default()),
                    }]
                } else {
                    g.remove(0)
                }
            };
            let mut sampling: Option<SamplingResult> = None;
            for c in chunks {
                if let StreamChunk::IsDone { result } = &c {
                    sampling = Some((**result).clone());
                }
                on_delta(c);
            }
            let s = sampling.unwrap_or_default();
            Ok(CompletionResponse {
                id: "cap".into(),
                model: "capturing".into(),
                content: synthia_provider::Content::Single(ContentPart::Text(
                    synthia_provider::TextContent {
                        text: s.text.clone(),
                        cache_control: None,
                    },
                )),
                usage: s.usage.clone(),
                cached: false,
                stop_reason: s.stop_reason.clone(),
            })
        }

        async fn embed(
            &self,
            _texts: Vec<String>,
        ) -> Result<Vec<Vec<f64>>, Error> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn system_prompt_is_injected_into_llm_request_first_message() {
        // End-to-end: drive `ReActAgent::run` and inspect the actual
        // `CompletionRequest.messages` that reaches the provider.
        // The very first message MUST be a `Role::System` whose
        // text contains the configured `system_prompt` / descriptor
        // `instructions` followed by the assembled manifest
        // sections. This proves the prompt is not just stored on
        // the agent — it is the first thing the LLM sees, and the
        // manifest sections reach the provider too.
        const PROMPT: &str = "PROBE-SYSTEM-PROMPT-XYZ";

        let provider =
            Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: "ok".into(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage::default(),
                    ..Default::default()
                }),
            }]]));

        let agent = ReActAgent::with_options(
            provider.clone(),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            PROMPT.to_string(),
        );

        let mut stream = agent
            .run(
                crate::input::AgentInput::text("hi"),
                Arc::new(CancellationToken::new()),
            )
            .await;
        while let Some(_ev) = stream.next().await {}

        // Drain events so the spawned task completes.
        drop(stream);

        // Allow the spawned task a tick to finish (it must complete
        // because the loop terminated on the no-tool-call branch).
        for _ in 0..50 {
            if provider.call_count.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let captured = provider.captured.lock().await;
        assert!(
            !captured.is_empty(),
            "provider should have received at least one CompletionRequest",
        );
        let messages = &captured[0];
        assert!(
            !messages.is_empty(),
            "CompletionRequest.messages must be non-empty",
        );

        use synthia_provider::Role;
        assert_eq!(
            messages[0].role,
            Role::System,
            "first message must be the system prompt",
        );
        let sys_text = match &messages[0].content {
            synthia_provider::Content::Single(ContentPart::Text(t)) => {
                t.text.clone()
            }
            synthia_provider::Content::Multi(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        };
        assert!(
            sys_text.contains(PROMPT),
            "system prompt text must contain what was configured (got {sys_text:?})",
        );
        assert!(
            sys_text.contains("<identity>"),
            "assembler must inject the identity section",
        );
        // This test does not register any tools, so the
        // `<available_tools>` section is correctly absent (the
        // assembler drops empty manifest sections — see
        // `prompt::tests::empty_manifests_drop_their_section`).
        // The `<identity>` block being present is sufficient
        // evidence the manifest pipeline ran end-to-end.

        // The user prompt must follow the system prompt.
        if messages.len() >= 2 {
            assert_eq!(messages[1].role, Role::User);
        }
    }

    #[tokio::test]
    async fn empty_system_prompt_still_emits_assembled_identity() {
        // Regression guard: the prompt assembler MUST always
        // emit a non-empty system message whenever a descriptor
        // is present (which is every registered agent). Even an
        // empty `descriptor.instructions` is followed by the
        // identity section, which carries the agent's name,
        // kind, and version — so the LLM always sees who it is.
        //
        // The old "no system message when instructions empty"
        // rule was scoped to the bare `descriptor.instructions`
        // path; the assembler generalises it to "the assembled
        // system message must never be empty" while preserving
        // the observability benefit of always knowing the
        // agent's identity.
        use crate::{
            agent::descriptor::AgentDescriptor,
            prompt::PromptContext,
        };

        let provider =
            Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: "ok".into(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage::default(),
                    ..Default::default()
                }),
            }]]));

        let agent = ReActAgent::with_prompt_context(
            provider.clone(),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            // empty system prompt
            String::new(),
            Arc::new(PromptContext::default()),
        );

        // Force the descriptor back to a bare shape so the
        // assembler has nothing to render.
        let bare = AgentDescriptor {
            name: "bare".into(),
            description: String::new(),
            kind: "bare".into(),
            version: "0.0.0".into(),
            instructions: String::new(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            model_hint: None,
            handoffs: Vec::new(),
            handoff_hint: None,
            output_schema: None,
            owner: None,
            domain: None,

            persona: None,
            display_name: None,
        };
        let mut agent = agent;
        agent.descriptor_mut(bare);

        let mut stream = agent
            .run(
                crate::input::AgentInput::text("hi"),
                Arc::new(CancellationToken::new()),
            )
            .await;
        while let Some(_ev) = stream.next().await {}
        drop(stream);

        for _ in 0..50 {
            if provider.call_count.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let captured = provider.captured.lock().await;
        assert!(!captured.is_empty());
        use synthia_provider::Role;
        // Even with empty instructions the assembler emits the
        // identity section (containing the bare descriptor's
        // name/kind/version), so the first message is System.
        assert_eq!(
            captured[0][0].role,
            Role::System,
            "empty instructions + bare descriptor must still emit a non-empty system message via the identity section",
        );
        let sys_text = match &captured[0][0].content {
            synthia_provider::Content::Single(ContentPart::Text(t)) => {
                t.text.clone()
            }
            _ => String::new(),
        };
        assert!(
            sys_text.contains("<identity>"),
            "assembled system message must include the identity section"
        );
        assert!(
            sys_text.contains("`bare`"),
            "identity section must surface the agent's name"
        );
        assert!(!sys_text.is_empty(), "system message must never be empty",);
        assert_eq!(captured[0][1].role, Role::User);
    }

    #[tokio::test]
    async fn run_stream_emits_session_ended_terminal() {
        let agent = ReActAgent::new(
            Arc::new(
                synthia_provider::traits_stub::ModelProviderStub::text_only(""),
            ),
            Arc::new(ToolRegistry::new()),
        );
        let mut stream = agent
            .run(AgentInput::text(""), Arc::new(CancellationToken::new()))
            .await;
        let mut saw_end = false;
        while let Some(ev) = stream.next().await {
            if matches!(
                ev,
                AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                })
            ) {
                saw_end = true;
                break;
            }
        }
        assert!(saw_end, "expected SessionEnded(Completed)");
    }

    // -- Loop-level tests (mirror the previous `run.rs` suite) ------------

    #[tokio::test]
    async fn run_emits_full_event_lifecycle_for_text_only_response() {
        let provider = Arc::new(ScriptedStreamProvider::new(vec![vec![
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "hello ".to_string(),
                cache_control: None,
            })),
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "world".to_string(),
                cache_control: None,
            })),
            StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: String::new(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                        cached_prompt_tokens: None,
                        cache_read_tokens: None,
                        cache_write_tokens: None,
                    },
                    ..Default::default()
                }),
            },
        ]]));
        let events = run_and_collect(
            provider,
            Arc::new(ToolRegistry::new()),
            CancellationToken::new(),
            AgentInput::text("hello"),
        )
        .await;

        // SessionStarted → Progress → Model(text) → Model(text) →
        // Usage → ModelDone → SessionEnded.
        let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                "System",
                "System",
                "Model",
                "Model",
                "System",
                "ModelDone",
                "System",
            ]
        );

        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            })
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::Progress { .. })
        )));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ModelDone(_))));
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::Usage {
                input_tokens: 10,
                output_tokens: 5,
                ..
            })
        )));
    }

    #[tokio::test]
    async fn isdone_text_does_not_duplicate_streamed_chunks() {
        // Real Anthropic / OpenAI providers stream text via
        // `Content(Text)` chunks AND include the consolidated
        // final text in `IsDone.result.text`. Without dedup
        // the consumer would see the same text twice (once
        // per chunk, once via IsDone). This test reproduces
        // that exact pattern and asserts the wire sees each
        // text fragment exactly once.
        let provider = Arc::new(ScriptedStreamProvider::new(vec![vec![
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "hello ".to_string(),
                cache_control: None,
            })),
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "world".to_string(),
                cache_control: None,
            })),
            StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    // Same text as the streamed chunks — the
                    // canonical duplicate.
                    text: "hello world".to_string(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage {
                        prompt_tokens: 1,
                        completion_tokens: 2,
                        total_tokens: 3,
                        cached_prompt_tokens: None,
                        cache_read_tokens: None,
                        cache_write_tokens: None,
                    },
                    ..Default::default()
                }),
            },
        ]]));

        let events = run_and_collect(
            provider,
            Arc::new(ToolRegistry::new()),
            CancellationToken::new(),
            AgentInput::text("hi"),
        )
        .await;

        let text_events: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::Text(tc)) => {
                    Some(tc.text.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            text_events,
            vec!["hello ".to_string(), "world".to_string()],
            "IsDone text must NOT be re-emitted as a wire event when streamed chunks already covered it; got {text_events:?}"
        );
    }

    #[tokio::test]
    async fn isdone_text_emitted_only_when_no_streamed_chunks() {
        // Mirror of the previous test: when the provider
        // batches everything into IsDone (no streamed chunks),
        // the IsDone text MUST surface on the wire — otherwise
        // non-streaming providers (or streaming providers with
        // no content events) would produce a silent assistant
        // turn.
        let provider = Arc::new(ScriptedStreamProvider::new(vec![vec![
            StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: "batched-response".to_string(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage::default(),
                    ..Default::default()
                }),
            },
        ]]));

        let events = run_and_collect(
            provider,
            Arc::new(ToolRegistry::new()),
            CancellationToken::new(),
            AgentInput::text("hi"),
        )
        .await;

        let text_events: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::Text(tc)) => {
                    Some(tc.text.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            text_events,
            vec!["batched-response".to_string()],
            "non-streaming IsDone must surface its text on the wire; got {text_events:?}"
        );
    }

    #[tokio::test]
    async fn isdone_tool_calls_dedup_against_streamed_tool_call_end() {
        // Anthropic and OpenAI both emit `ToolCallStart` +
        // `ToolCallEnd` AND re-list the same tool calls in
        // `IsDone.result.tool_calls`. Without dedup, the
        // history message would carry two identical
        // ContentPart::ToolUse entries for the same id —
        // corrupting the LLM context. This test asserts each
        // tool_use_id appears on the wire exactly once.
        let tool_use = ToolUse {
            id: "call_dup".to_string(),
            name: "noop".to_string(),
            input: json!({}),
        };
        // Patch the IsDone to also include the same tool
        // call (re-listing). We rebuild the chunks directly
        // because `tool_call_response` emits
        // `TokenUsage::default()` in IsDone — fine, but we
        // need tool_calls populated too.
        let chunks: Vec<Vec<StreamChunk>> = vec![vec![
            StreamChunk::ToolCallStart {
                id: tool_use.id.clone(),
                name: tool_use.name.clone(),
                arguments: Value::String("{}".to_string()),
            },
            StreamChunk::ToolCallEnd {
                id: tool_use.id.clone(),
            },
            StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: String::new(),
                    // Same id as the streamed chunk — must be deduped.
                    tool_calls: vec![tool_use.clone()],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage::default(),
                    ..Default::default()
                }),
            },
        ]];
        let provider = Arc::new(ScriptedStreamProvider::new(chunks));

        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            FakeTool::new("noop", "done"),
        )));

        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("dedup"),
        )
        .await;

        let wire_tool_use_ids: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::ToolUse(tu)) => {
                    Some(tu.id.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            wire_tool_use_ids,
            vec!["call_dup"],
            "tool_use_id must appear on the wire exactly once even when IsDone re-lists it; got {wire_tool_use_ids:?}"
        );
    }

    #[tokio::test]
    async fn token_usage_emitted_per_iteration_can_be_aggregated() {
        // Verifies that `Usage` events are emitted **once per
        // LLM call** (per iteration), not just once for the
        // whole session, and that intermediate
        // `StreamChunk::Usage` deltas do **not** double-count
        // (Anthropic's `message_delta.usage` arrives before
        // `message.usage`; only the latter is authoritative).
        // The test runs a 2-iteration loop (tool call → final
        // empty response) and asserts exactly two distinct
        // Usage events fire with the expected values per
        // iteration, even though the provider scripts
        // intermediate Usage chunks too.
        let tool_use = ToolUse {
            id: "call_x".to_string(),
            name: "noop".to_string(),
            input: json!({}),
        };
        let final_text_response_with_usage = |input: usize, output: usize| {
            vec![
                // Intermediate delta — must NOT surface a
                // Usage event on its own.
                StreamChunk::Usage(TokenUsage {
                    prompt_tokens: 1000, // decoy; would
                    // pollute the sum if emitted
                    completion_tokens: 1000,
                    total_tokens: 2000,
                    cached_prompt_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                }),
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "thinking".into(),
                    cache_control: None,
                })),
                StreamChunk::IsDone {
                    result: Box::new(SamplingResult {
                        text: String::new(),
                        tool_calls: vec![],
                        reasoning: String::new(),
                        reasoning_signature: None,
                        usage: TokenUsage {
                            prompt_tokens: input,
                            completion_tokens: output,
                            total_tokens: input + output,
                            cached_prompt_tokens: None,
                            cache_read_tokens: None,
                            cache_write_tokens: None,
                        },
                        ..Default::default()
                    }),
                },
            ]
        };
        let tool_chunks = vec![
            tool_call_response_with_usage(
                vec![tool_use.clone()],
                TokenUsage {
                    prompt_tokens: 1000, // decoy delta
                    completion_tokens: 1000,
                    total_tokens: 2000,
                    cached_prompt_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                },
            ),
            final_text_response_with_usage(120, 30),
        ];
        let provider = Arc::new(ScriptedStreamProvider::new(tool_chunks));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            FakeTool::new("noop", "done"),
        )));

        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("iterate"),
        )
        .await;

        let usage_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::System(SystemEvent::Usage {
                    input_tokens,
                    output_tokens,
                    ..
                }) => Some((*input_tokens, *output_tokens)),
                _ => None,
            })
            .collect();
        assert_eq!(
            usage_events.len(),
            2,
            "expected one Usage event per LLM iteration, got {usage_events:?}"
        );
        // Iteration 1: IsDone carries default usage
        // (tool_call_response used `TokenUsage::default()` in
        // its IsDone), so this iteration's Usage event is
        // (0, 0).
        assert_eq!(usage_events[0], (0, 0), "iteration 1");
        // Iteration 2: IsDone carries (120, 30) — proves the
        // intermediate decoy `(1000, 1000)` was suppressed.
        assert_eq!(usage_events[1], (120, 30), "iteration 2");

        // Sanity check: the aggregate is the IsDone totals,
        // not a doubled sum including the decoys.
        let (sum_in, sum_out) = usage_events
            .iter()
            .fold((0usize, 0usize), |(i, o), (x, y)| (i + x, o + y));
        assert_eq!((sum_in, sum_out), (120, 30));
    }

    #[tokio::test]
    async fn run_executes_tool_and_emits_tool_result() {
        let tool_use = ToolUse {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            input: json!({}),
        };
        let provider = Arc::new(ScriptedStreamProvider::new(vec![
            tool_call_response(vec![tool_use.clone()]),
            empty_response(),
        ]));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            FakeTool::new("echo", "echoed"),
        )));

        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("run tool"),
        )
        .await;

        let tool_uses: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::ToolUse(tu)) => Some(tu.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].id, "call_1");
        assert_eq!(tool_uses[0].name, "echo");

        let tool_results: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                    Some(tr.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(tool_results.len(), 1);
        assert_eq!(tool_results[0].tool_use_id, "call_1");
        assert_eq!(tool_results[0].tool_name.as_deref(), Some("echo"));
        assert!(!tool_results[0].is_error.unwrap_or(true));
        let text = match &tool_results[0].content[0] {
            ContentPart::Text(t) => &t.text,
            _ => panic!("expected text content"),
        };
        assert_eq!(text, "echoed");

        assert!(matches!(
            events.last(),
            Some(AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            }))
        ));
    }

    #[tokio::test]
    async fn run_emits_tool_progress_for_streaming_tool() {
        /// Tool that yields one Progress item then a Result.
        struct ProgressTool;

        #[async_trait]
        impl Tool for ProgressTool {
            fn name(&self) -> &str {
                "streamer"
            }

            fn description(&self) -> &str {
                "yields progress"
            }

            fn parameters(&self) -> serde_json::Value {
                json!({"type": "object"})
            }

            async fn call(
                &self,
                _input: serde_json::Value,
                _ctx: &Context,
            ) -> ToolOutput {
                ToolOutput::text("done")
            }

            fn stream<'a>(
                &'a self,
                _input: serde_json::Value,
                _ctx: &'a Context,
            ) -> std::pin::Pin<
                Box<dyn futures::Stream<Item = StreamOutput> + Send + 'a>,
            > {
                use futures::stream;
                let s = stream::iter(vec![
                    StreamOutput::Progress(ToolOutput::text("halfway")),
                    StreamOutput::Result(ToolOutput::text("done")),
                ]);
                Box::pin(s)
            }
        }

        let tool_use = ToolUse {
            id: "call_1".to_string(),
            name: "streamer".to_string(),
            input: json!({}),
        };
        let provider = Arc::new(ScriptedStreamProvider::new(vec![
            tool_call_response(vec![tool_use]),
            empty_response(),
        ]));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            ProgressTool,
        )));

        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("stream"),
        )
        .await;

        let progress_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    AgentEvent::System(SystemEvent::ToolProgress { .. })
                )
            })
            .collect();
        assert_eq!(progress_events.len(), 1);
        if let AgentEvent::System(SystemEvent::ToolProgress {
            tool_name,
            call_id,
            ..
        }) = progress_events[0]
        {
            assert_eq!(tool_name, "streamer");
            assert_eq!(call_id, "call_1");
        }
    }

    #[tokio::test]
    async fn run_emits_warning_at_max_iterations() {
        let scripted: Vec<Vec<StreamChunk>> = (0..MAX_ITERATIONS + 2)
            .map(|_| {
                tool_call_response(vec![ToolUse {
                    id: "loop".to_string(),
                    name: "echo".to_string(),
                    input: json!({}),
                }])
            })
            .collect();
        let provider = Arc::new(ScriptedStreamProvider::new(scripted));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            FakeTool::new("echo", "ok"),
        )));

        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("loop forever"),
        )
        .await;

        let warning = events.iter().find_map(|e| match e {
            AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::Loop,
                message,
                ..
            }) => Some(message.clone()),
            _ => None,
        });
        assert!(warning.is_some(), "expected Loop warning");
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::MaxIterations,
            })
        )));
    }

    #[tokio::test]
    async fn run_emits_session_interrupted_on_cancel() {
        let provider = Arc::new(ScriptedStreamProvider::new(Vec::new()));
        let cancel = CancellationToken::new();
        cancel.cancel();

        let events = run_and_collect(
            provider,
            Arc::new(ToolRegistry::new()),
            cancel,
            AgentInput::text("hi"),
        )
        .await;

        let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
        assert_eq!(kinds, vec!["System", "System", "System"]);
        assert!(matches!(
            events[1],
            AgentEvent::System(SystemEvent::SessionInterrupted { .. })
        ));
        assert!(matches!(
            events[2],
            AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Cancelled,
            })
        ));
    }

    // ------------------------------------------------------------------
    // Multi-expert code-review scenarios — drive the full ReAct
    // loop and verify the assembled system prompt under realistic
    // multi-agent configurations built with `Coordinator::delegate`.
    //
    // The scenarios below share a single fixture
    // (`code-review-panel`) with 5 peer agents and 4 tools; each
    // scenario mutates one input and asserts the resulting
    // session events + LLM call count.
    // ------------------------------------------------------------------

    /// Build the canonical panel fixture:
    ///
    /// - 4 tools: `read_file`, `shell`, `list_skills`, `submit_vote`.
    /// - 3 skills: `summarize` (enabled), `audit` (enabled),
    ///   `deprecated` (disabled).
    /// - 5 peer agents with handoff hints: planner / critic /
    ///   redteam / judge / formatter.
    fn build_panel_fixture() -> (
        Vec<synthia_provider::ToolDefinition>,
        crate::prompt::PromptContext,
    ) {
        let tools = vec![
            tools_with("read_file", "Read a file from disk."),
            tools_with("shell", "Run a shell command."),
            tools_with("list_skills", "List enabled skills."),
            tools_with("submit_vote", "Cast a panel vote."),
        ];
        let ctx = crate::prompt::PromptContext::default()
            .with_skill("summarize", "Summarize text.")
            .with_skill("audit", "Audit for safety.")
            // "deprecated" was disabled in earlier revisions and
            // is therefore not pushed in by the caller; see
            // `PromptContext` doc § "skills".
            .with_agent(&peer_descriptor(
                "planner",
                "Plan the work.",
                Some("complex tasks"),
            ))
            .with_agent(&peer_descriptor(
                "critic",
                "Critique proposals.",
                Some("after planner"),
            ))
            .with_agent(&peer_descriptor("redteam", "Break the candidate.", None))
            .with_agent(&peer_descriptor("judge", "Aggregate votes.", None))
            .with_agent(&peer_descriptor(
                "formatter",
                "Format final output.",
                Some("after judge"),
            ));
        (tools, ctx)
    }

    /// Build a peer [`AgentDescriptor`] for the panel fixture.
    /// Only `name`, `description`, and `handoff_hint` are
    /// exercised by the prompt assembler, so every other
    /// field is filled with a minimal sentinel.
    fn peer_descriptor(
        name: &str,
        description: &str,
        handoff_hint: Option<&str>,
    ) -> AgentDescriptor {
        AgentDescriptor {
            name: name.into(),
            description: description.into(),
            kind: "panel".into(),
            version: "1.0.0".into(),
            instructions: "".into(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            model_hint: None,
            handoffs: Vec::new(),
            handoff_hint: handoff_hint.map(str::to_string),
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
        }
    }

    /// Build a Critic-role descriptor matching the panel fixture.
    fn build_critic_descriptor() -> AgentDescriptor {
        AgentDescriptor {
            name: "critic".into(),
            description: "Critic agent on code-review-panel".into(),
            kind: "critic".into(),
            version: "1.0.0".into(),
            instructions: "Review the proposer's plan.".into(),
            capabilities: vec!["tools".into()],
            tools: vec![],
            model_hint: None,
            handoffs: vec!["planner".into()],
            handoff_hint: Some("complex code tasks".into()),
            output_schema: None,
            owner: Some("synthia".into()),
            domain: Some("coding".into()),

            persona: Some("You are a rigorous senior reviewer.".into()),
            display_name: None,
        }
    }

    /// Drive one full ReAct loop and drain every
    /// [`AgentEvent`] through the channel. Returns the captured
    /// event sequence. The provider handle is also returned
    /// (via the `provider` parameter) so the caller can read
    /// `call_count` and `captured.messages` after the session
    /// ends.
    async fn drive_loop_with_provider(
        provider: Arc<CapturingProvider>,
        agent: ReActAgent,
        cancel: CancellationToken,
    ) -> Vec<AgentEvent> {
        let _ = provider; // kept alive via Arc clones inside the agent
        let mut stream = agent
            .run(AgentInput::text("review the plan"), Arc::new(cancel))
            .await;
        let mut events = Vec::new();
        while let Some(ev) = stream.next().await {
            events.push(ev);
        }
        drop(stream);
        // Give the spawned task a moment to drain.
        for _ in 0..80 {
            if provider.call_count.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        events
    }

    #[tokio::test]
    async fn mock_panel_clean_scenario_passes_through() {
        // The clean scenario: descriptor + tools + skills +
        // peer agents all consistent. The loop must drive at
        // least one LLM pass.
        let (_tools, ctx) = build_panel_fixture();
        let provider = Arc::new(CapturingProvider::new(vec![
            empty_response(), // pass 1: critic emits a critique (text only)
        ]));
        let mut agent = ReActAgent::with_options(
            provider.clone(),
            Arc::new(ToolRegistry::new()),
            PathBuf::new(),
            "PANEL_BASE".to_string(),
        );
        agent.descriptor_mut(build_critic_descriptor());
        agent.set_prompt_context(ctx);

        let events = drive_loop_with_provider(
            provider.clone(),
            agent,
            CancellationToken::new(),
        )
        .await;

        // LLM was called (clean prompt + critic mandate).
        assert!(
            provider.call_count.load(Ordering::SeqCst) >= 1,
            "clean panel scenario must call the LLM"
        );

        // Captured messages: the system message contains the
        // panel directive + tool manifest + skill manifest +
        // peer-agent manifest.
        let captured = provider.captured.lock().await;
        let sys_text = match &captured[0][0].content {
            synthia_provider::Content::Single(ContentPart::Text(t)) => {
                t.text.clone()
            }
            _ => panic!("expected single text content"),
        };
        // Tool names MUST NOT leak into the system prompt — they
        // ride the completion request's `tools` field instead.
        for name in ["read_file", "shell", "list_skills", "submit_vote"] {
            assert!(
                !sys_text.contains(&format!("`{name}`")),
                "tool `{name}` must NOT appear in the system prompt"
            );
        }
        // Skill manifest surfaces the 2 enabled skills but
        // not the disabled one.
        assert!(sys_text.contains("<name>summarize</name>"));
        assert!(sys_text.contains("<name>audit</name>"));
        assert!(
            !sys_text.contains("<name>deprecated</name>"),
            "disabled skill must not be advertised"
        );
        // Peer-agent manifest surfaces all 5 panel members.
        for name in ["planner", "critic", "redteam", "judge", "formatter"] {
            assert!(
                sys_text.contains(&format!("`{name}`")),
                "peer agent `{name}` must appear in the manifest"
            );
        }
        assert!(
            !sys_text.contains("Use only these tools:"),
            "tool grounding belongs on the runtime, not in the prompt"
        );
        // Session ended cleanly (no Error reason).
        let ended_clean = events.iter().any(|e| {
            matches!(
                e,
                AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                    ..
                })
            )
        });
        assert!(ended_clean, "clean scenario must end with Completed");
    }

    // -- Parallel tool execution coverage -------------------------------

    struct SleepTool;

    #[async_trait]
    impl synthia_tool::Tool for SleepTool {
        fn name(&self) -> &str {
            "sleep"
        }

        fn description(&self) -> &str {
            "sleeps for a configurable duration"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        fn mode(&self) -> synthia_tool::traits::ExecutionMode {
            synthia_tool::traits::ExecutionMode::Parallel
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _ctx: &Context,
        ) -> ToolOutput {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            ToolOutput::text("done")
        }
    }

    #[tokio::test]
    async fn parallel_safe_tools_run_concurrently() {
        let tool_use1 = ToolUse {
            id: "c1".into(),
            name: "sleep".into(),
            input: json!({}),
        };
        let tool_use2 = ToolUse {
            id: "c2".into(),
            name: "sleep".into(),
            input: json!({}),
        };
        let tool_use3 = ToolUse {
            id: "c3".into(),
            name: "sleep".into(),
            input: json!({}),
        };
        let provider = Arc::new(CapturingProvider::new(vec![
            tool_call_response(vec![tool_use1, tool_use2, tool_use3]),
            empty_response(),
        ]));
        let registry = Arc::new(ToolRegistry::new());
        registry
            .register_entry(synthia_tool::ToolEntry::new(Arc::new(SleepTool)));
        let started = std::time::Instant::now();
        let _events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("run"),
        )
        .await;
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(250),
            "3 parallel-safe sleep(100ms) tools must finish in <250ms, took {elapsed:?}"
        );
    }

    struct SlowFastTool(u64);

    #[async_trait]
    impl synthia_tool::Tool for SlowFastTool {
        fn name(&self) -> &str {
            if self.0 == 1 { "slow" } else { "fast" }
        }

        fn description(&self) -> &str {
            "speed"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        fn mode(&self) -> synthia_tool::traits::ExecutionMode {
            synthia_tool::traits::ExecutionMode::Parallel
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _ctx: &Context,
        ) -> ToolOutput {
            let d = if self.0 == 1 { 200 } else { 10 };
            tokio::time::sleep(std::time::Duration::from_millis(d)).await;
            ToolOutput::text(self.name().to_string())
        }
    }

    #[tokio::test]
    async fn parallel_safe_tool_emits_progress_and_result_in_llm_order() {
        let tool_use_slow = ToolUse {
            id: "slow".into(),
            name: "slow".into(),
            input: json!({}),
        };
        let tool_use_fast = ToolUse {
            id: "fast".into(),
            name: "fast".into(),
            input: json!({}),
        };
        let provider = Arc::new(CapturingProvider::new(vec![
            tool_call_response(vec![tool_use_slow, tool_use_fast]),
            empty_response(),
        ]));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            SlowFastTool(1),
        )));
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            SlowFastTool(2),
        )));
        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("o"),
        )
        .await;

        let progress: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::System(SystemEvent::ToolProgress {
                    tool_name,
                    ..
                }) => Some(tool_name.clone()),
                _ => None,
            })
            .collect();
        // ToolProgress is only emitted by tools that override
        // `stream()`; these synthetic tools use the default
        // call→Result path, so progress is empty here. The
        // meaningful invariant is ToolResult order below.
        let _ = progress;

        let results: Vec<Option<String>> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                    Some(tr.tool_name.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            results,
            vec![Some("slow".to_string()), Some("fast".to_string())],
            "ToolResult order must follow LLM tool-call order"
        );
    }

    static UNSAFE_RUNNING: AtomicUsize = AtomicUsize::new(0);
    static MAX_CONCURRENT_UNSAFE: AtomicUsize = AtomicUsize::new(0);

    struct CountingSafeTool;
    struct CountingUnsafeTool;

    #[async_trait]
    impl synthia_tool::Tool for CountingSafeTool {
        fn name(&self) -> &str {
            "safe"
        }

        fn description(&self) -> &str {
            "safe"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object"})
        }

        fn mode(&self) -> synthia_tool::traits::ExecutionMode {
            synthia_tool::traits::ExecutionMode::Parallel
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            ToolOutput::text("safe-done")
        }
    }

    #[async_trait]
    impl synthia_tool::Tool for CountingUnsafeTool {
        fn name(&self) -> &str {
            "unsafe"
        }

        fn description(&self) -> &str {
            "unsafe"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object"})
        }

        fn mode(&self) -> synthia_tool::traits::ExecutionMode {
            synthia_tool::traits::ExecutionMode::Sequential
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            let n = UNSAFE_RUNNING.fetch_add(1, Ordering::SeqCst) + 1;
            MAX_CONCURRENT_UNSAFE.fetch_max(n, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            UNSAFE_RUNNING.fetch_sub(1, Ordering::SeqCst);
            ToolOutput::text("unsafe-done")
        }
    }

    #[tokio::test]
    async fn unsafe_tools_run_serially_even_when_safe_present() {
        UNSAFE_RUNNING.store(0, Ordering::SeqCst);
        MAX_CONCURRENT_UNSAFE.store(0, Ordering::SeqCst);

        let tool_use_safe = ToolUse {
            id: "s".into(),
            name: "safe".into(),
            input: json!({}),
        };
        let tool_use_unsafe = ToolUse {
            id: "u".into(),
            name: "unsafe".into(),
            input: json!({}),
        };
        let provider = Arc::new(CapturingProvider::new(vec![
            tool_call_response(vec![tool_use_safe, tool_use_unsafe]),
            empty_response(),
        ]));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            CountingSafeTool,
        )));
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            CountingUnsafeTool,
        )));
        let _events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("m"),
        )
        .await;
        assert_eq!(
            MAX_CONCURRENT_UNSAFE.load(Ordering::SeqCst),
            1,
            "unsafe tool must never overlap with itself"
        );
    }

    /// Sequential-mode semantics: when one tool returns
    /// `is_error = true`, the round must abort and any
    /// remaining sequential calls must NOT execute. The
    /// downstream LLM still receives a synthetic
    /// `ToolResult` for every `tool_use_id` so its history
    /// is well-formed, but later calls are reported as
    /// "did not run" — they never invoked the tool.
    ///
    /// This pins both the abort-on-error behaviour and the
    /// synthetic-result message contract.
    #[tokio::test]
    async fn sequential_round_aborts_after_first_error_and_skips_remaining() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Counter proves later calls never run.
        static LATER_CALLS: AtomicUsize = AtomicUsize::new(0);
        static EARLY_CALLS: AtomicUsize = AtomicUsize::new(0);

        struct EarlyErrorTool;
        #[async_trait]
        impl synthia_tool::Tool for EarlyErrorTool {
            fn name(&self) -> &str {
                "early"
            }

            fn description(&self) -> &str {
                "early"
            }

            fn parameters(&self) -> serde_json::Value {
                json!({"type":"object"})
            }

            fn mode(&self) -> synthia_tool::traits::ExecutionMode {
                synthia_tool::traits::ExecutionMode::Sequential
            }

            async fn call(
                &self,
                _: serde_json::Value,
                _: &Context,
            ) -> ToolOutput {
                EARLY_CALLS.fetch_add(1, Ordering::SeqCst);
                // Use the public `error` helper so we get
                // the correct `metadata` / `truncated_by`
                // defaults; we then override `is_error =
                // Some(true)` on the returned value.
                let mut out = ToolOutput::error("boom");
                out.is_error = Some(true);
                out
            }
        }

        struct LaterTool;
        #[async_trait]
        impl synthia_tool::Tool for LaterTool {
            fn name(&self) -> &str {
                "later"
            }

            fn description(&self) -> &str {
                "later"
            }

            fn parameters(&self) -> serde_json::Value {
                json!({"type":"object"})
            }

            fn mode(&self) -> synthia_tool::traits::ExecutionMode {
                synthia_tool::traits::ExecutionMode::Sequential
            }

            async fn call(
                &self,
                _: serde_json::Value,
                _: &Context,
            ) -> ToolOutput {
                LATER_CALLS.fetch_add(1, Ordering::SeqCst);
                ToolOutput::text("later-ok")
            }
        }

        let early = ToolUse {
            id: "e".into(),
            name: "early".into(),
            input: json!({}),
        };
        let later = ToolUse {
            id: "l".into(),
            name: "later".into(),
            input: json!({}),
        };
        let provider = Arc::new(CapturingProvider::new(vec![
            tool_call_response(vec![early, later]),
            empty_response(),
        ]));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            EarlyErrorTool,
        )));
        registry
            .register_entry(synthia_tool::ToolEntry::new(Arc::new(LaterTool)));

        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("m"),
        )
        .await;

        // The "early" tool fired exactly once (abort happened
        // on its first error).
        assert_eq!(
            EARLY_CALLS.load(Ordering::SeqCst),
            1,
            "early tool must be invoked exactly once"
        );
        // The "later" tool NEVER fired (round aborted).
        assert_eq!(
            LATER_CALLS.load(Ordering::SeqCst),
            0,
            "later tool must NOT be invoked after early error"
        );

        // Wire side: both tool_use_ids must surface as
        // ToolResults so the LLM's history stays well-formed.
        let tool_results: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::ToolResult(tr)) => Some(tr),
                _ => None,
            })
            .collect();
        let ids: Vec<&str> = tool_results
            .iter()
            .map(|tr| tr.tool_use_id.as_str())
            .collect();
        assert!(
            ids.contains(&"e") && ids.contains(&"l"),
            "both tool_use_ids must appear on the wire; got {ids:?}"
        );

        // The "later" tool's synthetic result must carry
        // the neutral "did not produce a result" message
        // (NOT "cancelled" — that was the previous
        // misleading wording) so the LLM can tell it
        // missed the call without learning anything about
        // the earlier tool's failure.
        let later_result = tool_results
            .iter()
            .find(|tr| tr.tool_use_id == "l")
            .expect("'l' result must exist");
        let later_text = later_result
            .content
            .iter()
            .filter_map(|c| match c {
                ContentPart::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            later_text.contains("did not produce a result"),
            "synthetic message must use the new wording; got {later_text:?}"
        );
        assert!(
            !later_text.contains("cancelled"),
            "synthetic message must not leak the old 'cancelled' wording; got {later_text:?}"
        );
        assert!(
            later_result.is_error.unwrap_or(false),
            "synthetic result must be flagged as error so LLM sees it as failure"
        );
    }

    struct LongTool;
    #[async_trait]
    impl synthia_tool::Tool for LongTool {
        fn name(&self) -> &str {
            "long"
        }

        fn description(&self) -> &str {
            "long"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object"})
        }

        fn mode(&self) -> synthia_tool::traits::ExecutionMode {
            synthia_tool::traits::ExecutionMode::Parallel
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            ToolOutput::text("never")
        }
    }

    #[tokio::test]
    async fn parallel_call_respects_cancellation() {
        // Cancel must prevent the second LLM iteration from
        // starting; the first iteration's tool calls complete
        // (long ones at most ~800ms each, but they run in
        // parallel) and then the loop notices the cancelled
        // token before issuing the next sampling pass. Bound:
        // 800ms (parallel long tools) + 200ms slack + next-iter
        // detection = well under 3s.
        let tool_use_a = ToolUse {
            id: "a".into(),
            name: "long".into(),
            input: json!({}),
        };
        let tool_use_b = ToolUse {
            id: "b".into(),
            name: "long".into(),
            input: json!({}),
        };
        let provider = Arc::new(CapturingProvider::new(vec![
            tool_call_response(vec![tool_use_a, tool_use_b]),
            empty_response(),
        ]));
        let registry = Arc::new(ToolRegistry::new());
        registry
            .register_entry(synthia_tool::ToolEntry::new(Arc::new(LongTool)));
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_clone.cancel();
        });
        let started = std::time::Instant::now();
        let _events =
            run_and_collect(provider, registry, cancel, AgentInput::text("c"))
                .await;
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "session must abort promptly on cancel even with long parallel tools"
        );
    }

    #[tokio::test]
    async fn default_execution_mode_is_parallel() {
        // The Tool trait's mode() defaults to Parallel; this
        // verifies the contract every tool implicitly relies on.
        struct GenericTool;
        #[async_trait]
        impl synthia_tool::Tool for GenericTool {
            fn name(&self) -> &str {
                "g"
            }

            fn description(&self) -> &str {
                "g"
            }

            fn parameters(&self) -> serde_json::Value {
                json!({"type":"object"})
            }

            async fn call(
                &self,
                _: serde_json::Value,
                _: &Context,
            ) -> ToolOutput {
                ToolOutput::text("ok")
            }
        }
        assert_eq!(
            GenericTool.mode(),
            synthia_tool::traits::ExecutionMode::Parallel
        );
    }

    #[test]
    fn with_descriptor_installs_descriptor_verbatim() {
        // Regression test: `with_descriptor` used to call
        // `with_options` internally, which built a default
        // descriptor (name="agent", capabilities=["tools",
        // "streaming", "cancellation"], etc.) and then
        // shadowed it with the caller's descriptor. The wasted
        // allocation was benign but the field-by-field overwrite
        // hid a bug class — if `with_options` ever changed to
        // mutate the descriptor, callers would silently lose
        // their customizations. The fix bypasses `with_options`
        // and installs the caller's descriptor verbatim. Verify
        // every field round-trips.
        let custom = AgentDescriptor {
            name: "custom-judge".to_string(),
            description: "Test judge that aggregates votes".to_string(),
            kind: "judge".to_string(),
            version: "2.0.0".to_string(),
            instructions: "You are a strict judge".to_string(),
            capabilities: vec!["judging".to_string()],
            tools: vec!["foo".to_string()],
            model_hint: Some("gpt-x".to_string()),
            handoffs: vec!["agent".to_string()],
            handoff_hint: Some("Use as final aggregator".to_string()),
            output_schema: None,
            owner: Some("team-a".to_string()),
            domain: Some("review".to_string()),

            persona: Some("Skeptical auditor".to_string()),
            display_name: None,
        };

        let provider: Arc<dyn ModelProvider> =
            Arc::new(ScriptedStreamProvider::new(vec![]));
        let agent = ReActAgent::with_descriptor(
            provider,
            Arc::new(ToolRegistry::new()),
            PathBuf::from("/tmp"),
            custom.clone(),
            Arc::new(PromptContext::default()),
        );

        let got = agent.descriptor();
        assert_eq!(got.name, "custom-judge");
        assert_eq!(got.description, "Test judge that aggregates votes");
        assert_eq!(got.kind, "judge");
        assert_eq!(got.version, "2.0.0");
        assert_eq!(got.instructions, "You are a strict judge");
        assert_eq!(got.capabilities, vec!["judging".to_string()]);
        assert_eq!(got.tools, vec!["foo".to_string()]);
        assert_eq!(got.model_hint.as_deref(), Some("gpt-x"));
        assert_eq!(got.handoffs, vec!["agent".to_string()]);
        assert_eq!(
            got.handoff_hint.as_deref(),
            Some("Use as final aggregator")
        );
        assert_eq!(got.owner.as_deref(), Some("team-a"));
        assert_eq!(got.domain.as_deref(), Some("review"));
        assert_eq!(got.persona.as_deref(), Some("Skeptical auditor"));
    }

    /// After the panel refactor this test has no panel fields
    /// to assert against. We keep a placeholder to document
    /// that the clobbering behaviour is now a no-op (there are
    /// no defaults that could overwrite a caller field).
    #[test]
    fn with_descriptor_preserves_caller_fields_after_panel_removal() {
        let descriptor = AgentDescriptor {
            name: "judge".into(),
            description: "judge".into(),
            kind: "judge".into(),
            version: "1.0.0".into(),
            instructions: "judge".into(),
            capabilities: vec![],
            tools: vec![],
            model_hint: None,
            handoffs: vec![],
            handoff_hint: None,
            output_schema: None,
            owner: None,
            domain: None,
            persona: Some("Skeptical auditor".into()),
            display_name: None,
        };
        let provider: Arc<dyn ModelProvider> =
            Arc::new(ScriptedStreamProvider::new(vec![]));
        let agent = ReActAgent::with_descriptor(
            provider,
            Arc::new(ToolRegistry::new()),
            PathBuf::from("."),
            descriptor,
            Arc::new(PromptContext::default()),
        );
        assert_eq!(
            agent.descriptor().persona.as_deref(),
            Some("Skeptical auditor")
        );
        assert_eq!(agent.descriptor().name, "judge");
    }

    // -- Logging instrumentation coverage ------------------------------

    /// Capture `tracing` events emitted on this test thread so we can
    /// assert the loop's debug/info call sites fire. We use a custom
    /// `MakeWriter` writing to a `Vec<u8>` and let `fmt` render the
    /// default human-readable format — that gives us a stable
    /// substring to grep for (the message text) without depending on
    /// any third-party test-helper crate.
    struct LogCapture(Arc<TokioMutex<Vec<u8>>>);

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
        type Writer = LogCaptureWriter;

        fn make_writer(&'a self) -> Self::Writer {
            LogCaptureWriter(self.0.clone())
        }
    }

    struct LogCaptureWriter(Arc<TokioMutex<Vec<u8>>>);

    impl std::io::Write for LogCaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            // `tokio::sync::Mutex::blocking_lock` is not safe in an
            // async writer; the fmt layer's writer runs synchronously,
            // so a std `try_lock` (best-effort) is enough — we never
            // assert exact ordering, only that messages appear.
            if let Ok(mut g) = self.0.try_lock() {
                let _ = g.write(buf);
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn loop_logs_core_branches_with_structured_fields() {
        // Drive a 1-iteration loop: empty response (no tool calls).
        // We expect at least these `info!` log messages to fire:
        //
        // - `session start: preparing messages …`
        // - `step 2 sample_once: dispatching streaming completion …`
        // - `step 2 sample_once: streaming completion returned`
        // - `no tool calls returned by model; ending session with Completed`
        // - `session end: finalize()`
        //
        // (`debug!` lines — per-prepare and per-tool-inner — are
        // exercised by `loop_logs_execute_tools_branch_with_parallel_buckets`,
        // which raises the max level to DEBUG.)
        let buf = Arc::new(TokioMutex::new(Vec::<u8>::new()));
        let capture = LogCapture(buf.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_writer(capture)
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let provider = Arc::new(
            synthia_provider::traits_stub::ModelProviderStub::text_only("ok"),
        );
        let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()));
        let mut stream = agent
            .run(
                crate::input::AgentInput::text("hi"),
                Arc::new(CancellationToken::new()),
            )
            .await;
        while let Some(_ev) = stream.next().await {}
        drop(stream);

        // Wait for the spawned task to finish so the terminal
        // log lines (session end / sample_once / no tool calls)
        // have been written before we snapshot the buffer.
        let needle = b"session end: finalize";
        for _ in 0..200 {
            let has_terminal = buf
                .try_lock()
                .map(|g| g.windows(needle.len()).any(|w| w == needle))
                .unwrap_or(false);
            if has_terminal {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let captured = String::from_utf8(
            buf.try_lock().map(|g| g.clone()).unwrap_or_default(),
        )
        .unwrap_or_default();

        // Each substring below is the exact literal message string
        // passed to the corresponding `tracing` macro; if any of
        // these change without a matching test update, this test
        // fails loudly so the message-log contract is pinned.
        for needle in [
            "session start: preparing messages",
            "step 2 sample_once: dispatching streaming completion",
            "step 2 sample_once: streaming completion returned",
            "no tool calls returned by model",
            "session end: finalize",
        ] {
            assert!(
                captured.contains(needle),
                "expected log substring {needle:?} in captured output; got: {captured}"
            );
        }
    }

    #[tokio::test]
    async fn loop_logs_execute_tools_branch_with_parallel_buckets() {
        // Drive a 2-iteration loop: tool call (one parallel tool) →
        // empty response. We expect the `step 4 execute_tools:`
        // message to fire with the `parallel_count` and
        // `sequential_count` structured fields.
        let tool_use = ToolUse {
            id: "call_log".to_string(),
            name: "echo".to_string(),
            input: json!({}),
        };
        let chunks: Vec<Vec<StreamChunk>> =
            vec![tool_call_response(vec![tool_use]), empty_response()];
        let provider = Arc::new(ScriptedStreamProvider::new(chunks));
        let registry = Arc::new(ToolRegistry::new());
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            FakeTool::new("echo", "ok"),
        )));

        let buf = Arc::new(TokioMutex::new(Vec::<u8>::new()));
        let capture = LogCapture(buf.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_writer(capture)
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("log"),
        )
        .await;

        // Sanity: the loop did drive at least one tool call (so the
        // execute_tools branch must have fired).
        let any_tool_use = events
            .iter()
            .any(|e| matches!(e, AgentEvent::Model(ContentPart::ToolUse(_))));
        assert!(any_tool_use, "loop must have executed a tool");

        // Allow the spawned task a tick to drain.
        for _ in 0..50 {
            if buf.try_lock().map(|g| !g.is_empty()).unwrap_or(false) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let captured = String::from_utf8(
            buf.try_lock().map(|g| g.clone()).unwrap_or_default(),
        )
        .unwrap_or_default();

        for needle in [
            "step 4 execute_tools: bucketing by ExecutionMode",
            "step 4 execute_tools: bucketed; running Parallel via join_all",
            "step 4 execute_tools: all ToolResults committed to history",
            "execute_tool_inner: invoking tool",
            "execute_tool_inner: tool stream completed",
        ] {
            assert!(
                captured.contains(needle),
                "expected log substring {needle:?} in captured output; got: {captured}"
            );
        }
        // Structured fields render as `parallel_count=1` etc. in the
        // default fmt format; pin one to lock the field name shape.
        assert!(
            captured.contains("parallel_count=1"),
            "expected parallel_count=1 structured field; got: {captured}"
        );
        assert!(
            captured.contains("sequential_count=0"),
            "expected sequential_count=0 structured field; got: {captured}"
        );
    }
}
