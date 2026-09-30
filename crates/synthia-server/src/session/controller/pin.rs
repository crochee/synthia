//! Per-turn provider selection: the pin that travels with a
//! submitted operation.
//!
//! ## Why the pin rides on the operation
//!
//! The provider a run samples with lives in
//! [`RunDependencies`](super::RunDependencies), which is owned by the
//! controller's inner state and is not reachable from a route
//! handler. The one thing a route *can* hand the controller is an
//! operation, so the selection is submitted alongside it:
//!
//! ```text
//! route: resolve `model` ─▶ Arc<dyn ModelProvider>
//!                          │
//!                          ▼
//! SessionController::submit_with_provider(op, PinnedProvider::new(provider))
//!                          │
//!                          ▼
//! dispatch loop: park the pin on the inner state
//!                          │
//!                          ▼
//! maybe_start_run: consume the pin, build AgentRunConfig { provider: … }
//! ```
//!
//! The pin is *consumed* by the run it starts, which buys two
//! properties a sticky swap on the controller could not:
//!
//! - it is per turn, not per session — the next turn without a
//!   selection runs the deployment default again;
//! - two operations queued behind a running turn each keep their own
//!   provider, so neither can swap the other's provider out.
//!
//! `SessionController::submit` is unchanged and means "no pin": the
//! session's configured default provider runs the turn.

use std::sync::Arc;

use synthia::provider::traits::ModelProvider;

use super::ops::SessionOp;

/// A provider selected for one turn.
pub struct PinnedProvider {
    provider: Arc<dyn ModelProvider>,
}

impl PinnedProvider {
    /// Pin `provider` for the operation this is submitted with.
    pub fn new(provider: Arc<dyn ModelProvider>) -> Self {
        Self { provider }
    }

    pub(super) fn take(self) -> Arc<dyn ModelProvider> {
        self.provider
    }
}

/// One entry on the controller's operation channel: the operation
/// plus the turn's provider selection, when the submitter made one.
///
/// A single envelope rather than a second channel, so an operation and
/// its pin can never be reordered against each other.
pub(super) struct SubmittedOp {
    pub(super) op: SessionOp,
    pub(super) provider: Option<PinnedProvider>,
}

impl SubmittedOp {
    /// The turn's selection, if any. `None` keeps the session's
    /// configured default provider.
    pub(super) fn into_parts(
        self,
    ) -> (SessionOp, Option<Arc<dyn ModelProvider>>) {
        let provider = self.provider.map(PinnedProvider::take);
        (self.op, provider)
    }
}
