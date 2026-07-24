//! Streaming-chunk handling.
//!
//! Two public pieces:
//!
//! - [`StreamAccumulator`] — owns the per-chunk state
//!   (`text` / `tool_calls` / `reasoning` / `usage` /
//!   `tool_buffers` / `text_deltas`) and the big `match` on
//!   `StreamChunk`. Each provider chunk maps onto the
//!   accumulator state via [`StreamAccumulator::handle_chunk`].
//! - [`spawn_provider_task`] — kicks off the
//!   `complete_with_stream` task on `tokio::spawn` and returns
//!   the `JoinHandle` so the orchestrator can await it (and
//!   abort it on cancellation).
//!
//! # `ChunkOutcome`
//!
//! [`StreamAccumulator::handle_chunk`] returns a [`ChunkOutcome`]
//! instead of `bool` so future variants (e.g. `Fatal`) don't
//! require changing every call site — the orchestrator only
//! cares about `is_done()` today.
//!
//! Kept separate from [`super::core`] (the orchestrator),
//! [`super::truncate`] (the pre-LLM truncate step) and
//! [`super::fallback`] (the sync fallback safety net) so the
//! entire streaming protocol lives in one readable file.

use std::{collections::HashMap, sync::Arc};

use synthia_core::Error;
use synthia_provider::{
    TokenUsage as ProviderTokenUsage,
    traits::ModelProvider,
    types::{
        CompletionRequest,
        CompletionResponse,
        ContentPart,
        SamplingResult,
        StreamChunk,
        ToolUse,
    },
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// What the orchestrator should do after a chunk has been folded
/// into the accumulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkOutcome {
    /// Keep draining the channel.
    Continue,
    /// Authoritative end-of-stream (`IsDone`).
    Done,
    /// Legacy end-of-turn signal (`Stop`). Break the loop but do not
    /// treat it as an authoritative `IsDone`; the post-loop fallback
    /// logic will decide whether to call `complete()`.
    Stop,
}

impl ChunkOutcome {
    pub fn is_done(&self) -> bool {
        matches!(self, ChunkOutcome::Done)
    }

    pub fn should_break(&self) -> bool {
        matches!(self, ChunkOutcome::Done | ChunkOutcome::Stop)
    }
}

/// Per-call accumulator for the streaming LLM response.
pub(super) struct StreamAccumulator {
    text: String,
    tool_calls: Vec<ToolUse>,
    reasoning: String,
    usage: ProviderTokenUsage,
    tool_buffers: HashMap<String, ToolUse>,
    /// Text deltas observed in the order they arrived, so the agent
    /// loop can yield one `AgentEvent::LlmStreamDelta` per chunk.
    /// Stays empty when the provider never streams text in chunks.
    text_deltas: Vec<String>,
}

impl StreamAccumulator {
    pub(super) fn new() -> Self {
        Self {
            text: String::new(),
            tool_calls: Vec::new(),
            reasoning: String::new(),
            usage: ProviderTokenUsage::default(),
            tool_buffers: HashMap::new(),
            text_deltas: Vec::new(),
        }
    }

    /// Fold one provider chunk into the accumulator state.
    pub(super) fn handle_chunk(&mut self, chunk: StreamChunk) -> ChunkOutcome {
        match chunk {
            StreamChunk::Content(ContentPart::Text(tc)) => {
                self.text_deltas.push(tc.text.clone());
                self.text.push_str(&tc.text);
                ChunkOutcome::Continue
            }
            StreamChunk::Content(ContentPart::Reasoning(tc)) => {
                self.reasoning.push_str(&tc.text);
                ChunkOutcome::Continue
            }
            StreamChunk::Content(ContentPart::ToolUse(tu)) => {
                self.tool_calls.push(tu);
                ChunkOutcome::Continue
            }
            StreamChunk::Content(
                ContentPart::Image(_)
                | ContentPart::Audio(_)
                | ContentPart::ToolResult(_)
                | ContentPart::Resource(_),
            ) => {
                // Non-text, non-tool content types are not used by the
                // sampling-result contract. They are accepted from
                // providers (e.g. image previews) but ignored here.
                ChunkOutcome::Continue
            }
            StreamChunk::Usage(u) => {
                self.usage = u;
                ChunkOutcome::Continue
            }
            StreamChunk::ToolCallStart {
                id,
                name,
                arguments,
            } => {
                self.tool_buffers.insert(
                    id.clone(),
                    ToolUse {
                        id,
                        name,
                        input: arguments,
                    },
                );
                ChunkOutcome::Continue
            }
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                if let Some(buf) = self.tool_buffers.get_mut(&id) {
                    // Append the partial JSON fragment to the buffer's
                    // `input`. We treat `input` as a JSON string during
                    // accumulation; the provider layer that emitted this
                    // delta is expected to also emit a `ToolCallEnd`
                    // (possibly followed by an updated `ToolUse`) — if
                    // not, the `complete_with_stream` default impl falls
                    // back to `complete()`'s parsed result.
                    let existing = match &buf.input {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    buf.input = serde_json::Value::String(format!(
                        "{existing}{arguments_delta}"
                    ));
                }
                ChunkOutcome::Continue
            }
            StreamChunk::ToolCallEnd { id } => {
                if let Some(mut buf) = self.tool_buffers.remove(&id) {
                    // Try to parse accumulated JSON; fall back to string
                    // on parse failure (provider bug or partial JSON).
                    if let serde_json::Value::String(s) = &buf.input
                        && let Ok(parsed) =
                            serde_json::from_str::<serde_json::Value>(s)
                    {
                        buf.input = parsed;
                    }
                    self.tool_calls.push(buf);
                }
                ChunkOutcome::Continue
            }
            StreamChunk::Stop(_) => {
                // Some providers (legacy path) signal end-of-stream with
                // `Stop` instead of `IsDone`. Break out and let the
                // post-loop logic decide whether to fall back.
                ChunkOutcome::Stop
            }
            StreamChunk::IsDone { result } => {
                // Authoritative end-of-stream. The provider has already
                // accumulated text/tool_calls/reasoning/usage, but we
                // prefer the agent's own accumulators (which include
                // `Content` events that may not be folded into the
                // provider's result on every code path) and only fall
                // back to the provider's result when our accumulators
                // are empty.
                if self.text.is_empty() {
                    self.text = result.text.clone();
                }
                if self.tool_calls.is_empty() {
                    self.tool_calls = result.tool_calls.clone();
                }
                if self.reasoning.is_empty() {
                    self.reasoning = result.reasoning.clone();
                }
                if self.usage.total_tokens == 0 && self.usage.prompt_tokens == 0
                {
                    self.usage = result.usage.clone();
                }
                ChunkOutcome::Done
            }
        }
    }

    /// Fill any empty accumulator field from a `SamplingResult`
    /// (used by the sync fallback path). Mirrors the
    /// "prefer own accumulators, fall back to provider result"
    /// rule from the `IsDone` arm.
    pub(super) fn fill_from_sampling(&mut self, sampling: SamplingResult) {
        if self.text.is_empty() {
            self.text = sampling.text;
        }
        if self.tool_calls.is_empty() {
            self.tool_calls = sampling.tool_calls;
        }
        if self.reasoning.is_empty() {
            self.reasoning = sampling.reasoning;
        }
        if self.usage.total_tokens == 0 && self.usage.prompt_tokens == 0 {
            self.usage = sampling.usage;
        }
    }

    /// Consume the accumulator and produce the agent-facing
    /// `(SamplingResult, Vec<String>)` return value.
    pub(super) fn finalize(self) -> (SamplingResult, Vec<String>) {
        (
            SamplingResult {
                text: self.text,
                tool_calls: self.tool_calls,
                reasoning: self.reasoning,
                usage: self.usage,
            },
            self.text_deltas,
        )
    }
}

/// Spawn the `complete_with_stream` task. The channel is bounded;
/// the agent loop's drain speed is the natural backpressure — the
/// provider callback's `try_send` will return `Full` if the agent
/// is slow, and the final `IsDone` chunk is what actually
/// converges the call.
pub(super) fn spawn_provider_task(
    provider: Arc<dyn ModelProvider>,
    request: CompletionRequest,
    cancel_token: CancellationToken,
    tx: mpsc::Sender<StreamChunk>,
) -> JoinHandle<Result<CompletionResponse, Error>> {
    tokio::spawn(async move {
        provider
            .complete_with_stream(
                request,
                Some(cancel_token),
                Box::new(move |chunk| {
                    let _ = tx.try_send(chunk);
                }),
            )
            .await
    })
}
