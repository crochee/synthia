//! Builder-pattern accessors and `with_*` setters for
//! [`StreamBuilder`].
//!
//! Three setters
//! ([`StreamBuilder::with_prefix_tracker`],
//! [`StreamBuilder::on_prefix_event`],
//! [`StreamBuilder::with_initial_state`]) and three
//! read accessors ([`StreamBuilder::context`],
//! [`StreamBuilder::hooks`],
//! [`StreamBuilder::hooks_mut`]).
//!
//! `with_prefix_tracker` and `on_prefix_event` return
//! `Self` for the consuming-style fluent chain.
//! `with_initial_state` returns `&mut Self` so callers
//! can mutate the builder and immediately call `run()`
//! on the borrowed reference.

use std::sync::Arc;

use parking_lot::Mutex;
use synthia_context::{
    assembler::ContextAssembler,
    prefix_tracker::{PrefixStabilityEvent, PrefixTracker},
};
use synthia_provider::types::Message;

use crate::stream_builder::{
    builder::types::StreamBuilder,
    hook_builder::HookBuilder,
};

impl StreamBuilder {
    /// Borrow the session's [`ContextAssembler`].
    pub fn context(&self) -> &ContextAssembler {
        &self.context
    }

    /// Borrow the session's [`HookBuilder`].
    pub fn hooks(&self) -> &HookBuilder {
        &self.hooks
    }

    /// Mutably borrow the session's [`HookBuilder`].
    pub fn hooks_mut(&mut self) -> &mut HookBuilder {
        &mut self.hooks
    }

    /// Replace the internal `PrefixTracker` (used for sharing across sessions
    /// or injecting a pre-configured tracker in tests).
    pub fn with_prefix_tracker(
        mut self,
        tracker: Arc<Mutex<PrefixTracker>>,
    ) -> Self {
        self.prefix_tracker = tracker;
        self
    }

    /// Register a callback invoked after each LLM call with the
    /// `PrefixStabilityEvent` carrying `turn_id` and `stability_ratio`.
    /// Callback must outlive the stream (typically `'static`).
    pub fn on_prefix_event(
        mut self,
        cb: Arc<dyn Fn(PrefixStabilityEvent) + Send + Sync + 'static>,
    ) -> Self {
        self.on_prefix_event = Some(cb);
        self
    }

    /// Seed the loop with a checkpoint-resumed `(messages, iteration)`
    /// tuple. Returns `&mut Self` so callers can immediately `.run(...)`
    /// the same builder reference.
    pub fn with_initial_state(
        &mut self,
        messages: Vec<Message>,
        iteration: usize,
    ) -> &mut Self {
        self.initial_state = Some((messages, iteration));
        self
    }
}
