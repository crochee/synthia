//! `StepSample` — the streaming LLM-call step in the ReAct loop.
//!
//! Wraps the `ModelProvider::complete_with_stream` callback into a
//! single `execute()` method that returns a [`SamplingResult`] plus
//! the per-chunk text deltas (so the agent loop can yield
//! `AgentEvent::LlmStreamDelta` events in order).
//!
//! # Pipeline
//!
//! 1. [`super::truncate::truncate_tool_messages`] — destructive
//!    in-place truncation of `Tool` role messages using the unified
//!    `synthia_context::truncate` service (head/tail + disk spill).
//! 2. [`super::request::build_completion_request`] — assemble the
//!    `CompletionRequest` from `AgentConfig` + the current
//!    `LoopContext` messages.
//! 3. [`super::stream::spawn_provider_task`] — kick off the
//!    `complete_with_stream` task on `tokio::spawn`. The task
//!    forwards `StreamChunk`s through a bounded mpsc channel
//!    (backpressure: the channel is the bounded point; the agent
//!    loop drains at its own pace).
//! 4. [`super::stream::StreamAccumulator`] — owns the per-chunk
//!    state (`text` / `tool_calls` / `reasoning` / `usage` /
//!    `tool_buffers` / `text_deltas`) and the big `match` on
//!    `StreamChunk`. The match is the single place that knows how
//!    each provider chunk maps onto the agent's accumulator
//!    state.
//! 5. [`super::fallback::synchronous_fallback`] — when the stream
//!    closes without an authoritative `IsDone`, fall back to a
//!    single `provider.complete()` call to keep the agent making
//!    progress.
//!
//! Kept separate from the other step implementations
//! (`StepCompact` / `StepReflect` / `StepSpawn` /
//! `StepToolExecute`) — this one is the largest because it owns
//! the full streaming contract, the other steps just orchestrate
//! pre-existing helpers.

use std::sync::Arc;

use synthia_context::truncate::TruncateConfig;
use synthia_core::Error;
use synthia_provider::{
    traits::ModelProvider,
    types::{SamplingResult, StreamChunk},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::{fallback, request, stream, truncate};
use crate::{config::AgentConfig, loop_context::LoopContext};

/// Default capacity of the channel that carries `StreamChunk`s from the
/// provider callback into the agent's main loop. Acts as backpressure: when
/// the agent is busy, the provider blocks at `try_send` and waits for the
/// agent to drain.
pub(super) const STREAM_CHANNEL_CAPACITY: usize = 64;

pub struct StepSample {
    config: AgentConfig,
    /// Truncate configuration applied to `Tool` role messages in the
    /// outgoing context (head/tail + disk spill). Defaults are tuned by
    /// `TruncateConfig::default()`.
    truncate_cfg: TruncateConfig,
}

impl StepSample {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            truncate_cfg: TruncateConfig::default(),
        }
    }

    /// Replace the truncate configuration (used by tests + config hot-reload).
    pub fn with_truncate_config(mut self, cfg: TruncateConfig) -> Self {
        self.truncate_cfg = cfg;
        self
    }

    pub async fn execute(
        &self,
        provider: Arc<dyn ModelProvider>,
        ctx: &mut LoopContext,
        tools: Vec<synthia_provider::ToolDefinition>,
        cancel_token: CancellationToken,
    ) -> Result<(SamplingResult, Vec<String>), Error> {
        truncate::truncate_tool_messages(&mut ctx.messages, &self.truncate_cfg);

        let request =
            request::build_completion_request(&self.config, ctx, tools);

        // Single-flight streaming attempt. The channel carries chunks from
        // the provider callback to the agent's main loop, giving the agent
        // full ownership of cancellation + fallback.
        let (tx, mut rx) =
            mpsc::channel::<StreamChunk>(STREAM_CHANNEL_CAPACITY);
        let provider_task = stream::spawn_provider_task(
            provider.clone(),
            request.clone(),
            cancel_token.clone(),
            tx,
        );

        let mut accumulator = stream::StreamAccumulator::new();
        let mut is_done_received = false;
        let mut last_error: Option<Error> = None;

        while let Some(chunk) = rx.recv().await {
            // Cooperative cancellation: abort the provider task so its
            // HTTP stream is dropped and we don't wait for the full
            // request to drain. The provider's own `complete_with_stream`
            // also polls the same `cancel_token` (5s grace period), so
            // an abort here is the fast path. The caller's recovery
            // layer decides whether to retry.
            if cancel_token.is_cancelled() {
                provider_task.abort();
                return Err(Error::Provider("Request cancelled".to_string()));
            }

            let outcome = accumulator.handle_chunk(chunk);
            if outcome.is_done() {
                is_done_received = true;
                break;
            }
            if outcome.should_break() {
                break;
            }
        }

        // Wait for the provider task to finish (it has either returned
        // already, or will return shortly after the channel is dropped).
        match provider_task.await {
            Ok(Ok(_response)) => {}
            Ok(Err(e)) => {
                // Provider surfaced an error after we already drained. If
                // we have a `SamplingResult` from `IsDone`, prefer it; the
                // error is recorded for observability only.
                if !is_done_received {
                    last_error = Some(e);
                }
            }
            Err(join_err) => {
                if !is_done_received {
                    last_error = Some(Error::Provider(join_err.to_string()));
                }
            }
        }

        // Stream closed without an authoritative `IsDone` → fall back to a
        // single synchronous `complete()` call. This is the safety net that
        // keeps the agent making progress even when the streaming path is
        // broken (provider bug, mid-flight HTTP disconnect, etc.).
        let accumulator = if !is_done_received {
            warn!(
                target: "synthia.agent.step_sample",
                "stream_closed_early: provider stream ended without IsDone; \
                 falling back to synchronous complete()"
            );
            metrics::counter!("synthia_stream_closed_early_total").increment(1);
            fallback::synchronous_fallback(provider, request, accumulator)
                .await?
        } else {
            accumulator
        };

        if let Some(e) = last_error {
            return Err(e);
        }

        Ok(accumulator.finalize())
    }
}
