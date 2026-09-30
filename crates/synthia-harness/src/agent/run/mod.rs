//! Driving an [`Agent`](super::Agent) run — the handle a caller keeps on
//! one, and what it can do while that run is in flight.
//!
//! Two seams, one concern. Both answer "what can the outside world do to
//! a run that is already moving?", which is why they sit together rather
//! than next to the loop they steer:
//!
//! - [`handle`] — [`AgentHandle::spawn_detached`](handle::AgentHandle::spawn_detached)
//!   hands back a [`DetachedAgent`](handle::DetachedAgent) the caller can
//!   join, poll, or abandon without cancelling the work. The run starts
//!   on the first join, not at `spawn_detached` — the handle holds no
//!   executor, so it cannot start one itself.
//! - [`inbox`] — [`RunInbox`](inbox::RunInbox) lets a front end push
//!   messages *into* an in-flight run: steering between tool rounds, and
//!   follow-ups that revive a run about to stop.
//!
//! Neither knows which strategy is running, and neither is needed to run
//! an agent: a caller that drives `Agent::run` itself uses neither.

mod handle;
mod inbox;

pub use handle::{AgentHandle, DetachedAgent, DetachedClosed, DetachedError};
pub use inbox::{MpscInbox, RunInbox, RunInboxHandle};

#[cfg(test)]
mod tests;
