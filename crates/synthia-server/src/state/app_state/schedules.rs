//! Schedules state: wraps [`ScheduleStore`] (disk-backed) and
//! exposes the operations the `/api/v1/schedules/*` handlers need.
//!
//! The store persists jobs to `<workspace>/schedules/<id>.json`
//! under PID-locked atomic writes (`crates/synthia-scheduler/src/store.rs`).
//! Restart picks the rows back up via `ScheduleStore::load`.

use std::sync::Arc;

use synthia::{
    core::Clock as _,
    scheduler::{Job, ScheduleStore, Scheduler},
};

/// `SchedulesState` — the schedule subsystem's shared handle.
///
/// `store` is the durable layer (`ScheduleStore`) and `scheduler`
/// is the runtime-neutral `Scheduler` that the server uses for
/// tick operations. `Arc` so both can be cloned into the route
/// handlers cheaply.
#[derive(Clone)]
pub struct SchedulesState {
    pub store: Arc<ScheduleStore>,
    pub scheduler: Arc<Scheduler>,
    /// Wall clock the tick reads "now" from (AGENTS.md §3.8: a
    /// library never calls `Utc::now()` itself — the deployment's
    /// one clock is injected, so a test can pin time).
    clock: synthia::core::SharedClock,
}

impl SchedulesState {
    /// Build the state from a workspace root. The store's directory
    /// is `<root>/schedules`; `load` is called once to pick up any
    /// jobs the previous process left behind. A load failure is
    /// logged but does not panic — the operator sees a warning and
    /// the empty directory is the operational state.
    pub fn build(workspace_root: &std::path::Path) -> Self {
        Self::build_with_clock(
            workspace_root,
            synthia::core::SharedClock::system(),
        )
    }

    /// [`Self::build`] with an explicit clock — the seam a test
    /// uses to pin "now" and make a due job deterministic.
    pub fn build_with_clock(
        workspace_root: &std::path::Path,
        clock: synthia::core::SharedClock,
    ) -> Self {
        let store = Arc::new(ScheduleStore::new(workspace_root));
        match store.load() {
            Ok(n) => tracing::info!(
                "schedules: loaded {n} job(s) from {}",
                workspace_root.display()
            ),
            Err(e) => tracing::warn!(
                "schedules: load failed ({e}); starting with empty store"
            ),
        }
        let scheduler = Arc::new(Scheduler::new(Arc::clone(&store)));
        Self {
            store,
            scheduler,
            clock,
        }
    }

    /// List every job (in id order). Cheap — backed by the in-memory cache.
    pub fn list(&self) -> Vec<Job> {
        self.store.list()
    }

    /// One job by id. `None` when no such id exists.
    pub fn get(&self, id: &synthia::scheduler::JobId) -> Option<Job> {
        self.store.get(id)
    }

    /// Add a new job. Returns the persisted `Job` on success.
    pub fn add(&self, job: Job) -> Result<(), String> {
        self.store.add(job).map_err(|e| e.to_string())
    }

    /// Mutate a job in place. Returns the new `Job` after the
    /// mutation, or `None` when no job with that id exists.
    pub fn update<F>(
        &self,
        id: &synthia::scheduler::JobId,
        mutator: F,
    ) -> Result<Option<Job>, String>
    where
        F: FnOnce(&mut Job),
    {
        self.store.update(id, mutator).map_err(|e| e.to_string())
    }

    /// Remove a job by id. Returns true if it existed.
    pub fn remove(
        &self,
        id: &synthia::scheduler::JobId,
    ) -> Result<bool, String> {
        self.store.remove(id).map_err(|e| e.to_string())
    }

    /// Drive a single tick and return the jobs that fired.
    pub fn tick(&self) -> Vec<synthia::scheduler::scheduler::FiredJob> {
        let now = self.clock.now();
        self.scheduler.tick(now).fired
    }

    /// The state's wall clock, for handlers that must read the same
    /// "now" the tick does.
    #[must_use]
    pub fn clock(&self) -> &synthia::core::SharedClock {
        &self.clock
    }
}
