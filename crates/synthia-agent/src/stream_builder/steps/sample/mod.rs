//! `StepSample` — the streaming LLM-call step in the ReAct loop.
//!
//! The original 737-line `sample.rs` was split into focused
//! submodules by responsibility:
//!
//! - [`core`]: the [`StepSample`] struct +
//!   [`core::STREAM_CHANNEL_CAPACITY`] constant + the public
//!   `execute()` orchestrator that ties everything together.
//! - [`truncate`]: pre-LLM tool-message truncation using the
//!   unified `synthia_context::truncate` service.
//! - [`request`]: `CompletionRequest` assembly from
//!   `AgentConfig` + `LoopContext`.
//! - [`stream`]: the [`stream::StreamAccumulator`] state machine
//!   over `StreamChunk`s + the [`stream::spawn_provider_task`]
//!   task factory.
//! - [`fallback`]: the synchronous `provider.complete()` safety
//!   net used when the stream closes without `IsDone`.
//!
//! The 6 unit tests live in [`tests`].

mod core;
mod fallback;
mod request;
mod stream;
mod truncate;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use core::StepSample;
