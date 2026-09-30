//! [`synthia_tool_scheduler`] — the `schedule` tool: create and manage
//! scheduled jobs from the model side.
//!
//! The [`scheduler`](crate::scheduler) feature ships the *building
//! block* — a [`ScheduleStore`](crate::scheduler::ScheduleStore) and a
//! [`Scheduler`](crate::scheduler::Scheduler) whose `tick(now)` the
//! host drives. This module is its model-facing adapter:
//! [`SchedulerTool`] is one [`Tool`](crate::tool::Tool) with five
//! actions (`list` / `create` / `pause` / `resume` / `remove`), so an
//! agent can read and edit the schedule without the host hand-coding a
//! tool per verb.
//!
//! ```rust,ignore
//! use std::sync::Arc;
//!
//! use synthia::prelude::*;
//! use synthia::scheduler::ScheduleStore;
//! use synthia::tool_scheduler::{SchedulerTool, register_scheduler_tool};
//!
//! // One store, shared: the tool writes rows, the host's scheduler
//! // tick delivers them.
//! let store = Arc::new(ScheduleStore::new(".synthia/schedules"));
//! let registry = ToolRegistry::new();
//! register_scheduler_tool(&registry, SchedulerTool::new(Arc::clone(&store)));
//!
//! let scheduler = synthia::scheduler::Scheduler::new(store);
//! // Drive the schedule from the consumer's own timer: a tool action
//! // writes the store directly, and the next pass notices it.
//! scheduler.tick(now);
//! ```
//!
//! The tool never fires a job: it plans, the host delivers. That split
//! is the whole design — see the crate docs for the delivery boundary
//! and for why `cron` is opt-in (the `cron` feature).
//!
//! **A write reaches the next pass on its own.** Every action here
//! writes the store directly, and the wheel `Scheduler` ticks off
//! notices: each pass compares the store's revision with the one its
//! wheel was built from and rebuilds when they differ. Nothing has to
//! be reconciled by hand.
//!
//! [`Scheduler::resync`](crate::scheduler::Scheduler::resync) remains
//! as a retained explicit-rebuild escape hatch: it forces the rebuild at
//! an instant of the caller's choosing, rather than waiting for the next
//! pass to notice:
//!
//! ```ignore
//! scheduler.resync(now); // explicit rebuild; the next tick would do it too
//! ```
//!
//! ```bash
//! cargo run --example schedule_jobs -p synthia-tool-scheduler
//! ```

pub use synthia_tool_scheduler::*;
