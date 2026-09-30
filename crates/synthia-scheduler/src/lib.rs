//! `synthia-scheduler` — subagent scheduling dispatcher.
//!
//! Adopted from pi-subagents `src/schedule.ts` +
//! `src/schedule-store.ts`. Synthia ports the parts that are
//! worth keeping and ditches the parts that conflict with
//! rustc idioms:
//!
//! - **Cron** is a `JobTrigger`, not a free-form string. The default
//!   build ships no cron parser: a `JobKind::Cron` job's `advance`
//!   re-arms it one minute out, and the `schedule` tool layer keeps
//!   refusing `create kind=cron`. With the `cron` feature on,
//!   `CronTrigger` (5-field POSIX) owns the real recurrence and that
//!   refusal lifts.
//! - **PID-locked atomic JSON persistence** is preserved
//!   verbatim: a `<root>/schedules/<job_id>.json` per job,
//!   `flock` for cross-instance mutual exclusion, temp +
//!   rename for atomic writes. Reload-on-boot picks up jobs
//!   the previous process left behind.
//! - **Runtime neutrality** (no `tokio` in the public API):
//!   `Scheduler::tick(now) -> Vec<JobId>` returns the jobs
//!   whose fire-time has arrived; the caller drives the
//!   scheduler from whatever executor it uses. This is the
//!   same discipline the rest of synthia's library crates
//!   follow.
//!
//! The scheduler is a building block — it does not spawn
//! agents, send prompts, or talk to the session crate.
//! Callers wire `Job::on_fire` to whatever delivery path
//! they already own (the existing `GroupJoin` /
//! `subagent-notification` flow in pi-subagents is the
//! reference consumer).

pub mod job;
pub mod scheduler;
pub mod store;
pub mod trigger;
pub mod wheel;

pub use job::{
    Job,
    JobId,
    JobKind,
    JobStatus,
    MAX_INTERVAL_SECS,
    ScheduleError,
};
pub use scheduler::{ScheduleTick, Scheduler};
pub use store::{ScheduleStore, ScheduleStoreError};
#[cfg(feature = "cron")]
pub use trigger::CronTrigger;
pub use trigger::{IntervalTrigger, JobTrigger, OnceTrigger};
pub use wheel::TimingWheel;
