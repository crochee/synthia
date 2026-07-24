//! Synchronous `complete()` fallback path.
//!
//! When the streaming call closes without an authoritative
//! `IsDone` (provider bug, mid-flight HTTP disconnect, etc.) the
//! agent still needs a `SamplingResult` to make progress. The
//! fallback is a single `provider.complete(request)` call — the
//! same `CompletionRequest` that was sent to the streaming task,
//! reused so the truncated context, tool list, temperature and
//! model config are all identical.
//!
//! The result is folded back into the existing
//! [`super::stream::StreamAccumulator`] via its
//! `fill_from_sampling` method, which respects the same
//! "prefer own accumulators, fall back to provider result" rule
//! used by the `IsDone` arm.
//!
//! Kept separate from [`super::stream`] (the streaming contract)
//! and [`super::core`] (the orchestrator) so the fallback is one
//! self-contained helper that can be tested in isolation if
//! needed.

use std::sync::Arc;

use synthia_core::Error;
use synthia_provider::{
    traits::ModelProvider,
    types::{CompletionRequest, SamplingResult},
};
use tracing::warn;

use super::stream::StreamAccumulator;

/// Run the synchronous fallback and merge the result into the
/// (possibly partially-filled) accumulator. Returns the final
/// accumulator ready to be finalized by the caller.
pub(super) async fn synchronous_fallback(
    provider: Arc<dyn ModelProvider>,
    request: CompletionRequest,
    mut accumulator: StreamAccumulator,
) -> Result<StreamAccumulator, Error> {
    warn!(
        target: "synthia.agent.step_sample",
        "synchronous_fallback: invoking provider.complete() after stream \
         closed without IsDone"
    );
    let response = provider.complete(request).await?;
    let sampling: SamplingResult =
        synthia_provider::traits::completion_to_sampling(&response);
    accumulator.fill_from_sampling(sampling);
    Ok(accumulator)
}
