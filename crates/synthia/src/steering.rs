//! [`synthia_steering`] — everything that observes,
//! nudges, or vetoes a run without the agent loop knowing a policy.
//!
//! One container ([`Steering`]) holding five seams: [`Guard`]
//! (fail-closed veto), [`AgentHook`] (async lifecycle), [`Hint`]
//! (advisory injection), [`tracker::Tracker`] (sync observation +
//! concurrency advice), and [`OutputTransformer`] (tool-output
//! rewrite). [`Steering::noop`] is the empty default;
//! [`Steering::default_policy`] wires the defensive guards;
//! [`for_tier`] scales them to a
//! [`ModelTier`](crate::provider::ModelTier).

pub use synthia_steering::*;
