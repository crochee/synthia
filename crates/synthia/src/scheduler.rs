//! [`synthia_scheduler`] — the subagent scheduling
//! dispatcher.
//!
//! A [`Job`] carries a typed [`JobKind`] (`Once` / `Interval` /
//! `Cron`) and an `on_fire` payload; [`Scheduler`] answers
//! `tick(now)` with the jobs whose fire-time arrived, so the caller's
//! executor drives it (no timers, no runtime, no spawned tasks in the
//! library). Persistence is one flock-guarded JSON file per job.
//!
//! This is the host half. The model-facing half is the `schedule` tool
//! in `synthia::tool_scheduler` (feature `tool-scheduler`): share one
//! `Arc<ScheduleStore>` between them and the agent reads and edits
//! exactly the schedule the host delivers.

pub use synthia_scheduler::*;
