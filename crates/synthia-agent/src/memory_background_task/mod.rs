//! Background task for processing memory events and periodic compaction.

mod spawn;
mod task;

#[cfg(test)]
mod tests;

pub use spawn::{default_shutdown_timeout, graceful_shutdown, spawn};
pub use task::MemoryBackgroundTask;
